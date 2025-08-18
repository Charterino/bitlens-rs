// This file implements the functionality necessary for the explorer to keep up with the chain.
// It's implemented in the following way:
//
// On startup, `start()` is called, which spawns 2 tasks:
// "worker manager" - manages the workers (peers) that sign up for work via `WORKER_REGISTRATION_CHANNEL`, dispatching jobs to them that are submitted by the chain watcher.
// "chain watcher" - generates jobs for the worker manager to dispatch and applies new headers and blocks in the correct order
//
// both of the tasks do essentially nothing until we start receiving headers/blocks via `on_header_received` and `on_block_received`,
// which are not called until the initial sync is complete.
//
// After the initial sync is complete, whenever we receive a `block` / `headers` packet from a remote peer (src/crawler/mod.rs), functions `on_header_received` and `on_block_received` from this file are called,
// which send messages to the chain watcher.
// When the chain watcher changes the top header, a message will be sent on `NEW_HEADER_BROADCAST`.
//
// When the chain watcher receives a new header that has more work than our current top header,
// it checks if it's parent is our current top header.
// if it is, then it's a simple chain extension, and it send a job to fetch that block's body to the worker manager.
// after it gets that body, it applies it and updates the frontpage stats by appending the new block + txs.
// if it is not, then it's a fork, and we need to figure out the last common block and reapply all blocks since that point,
// because a transaction might have moved from one block to another, e.g:
// old chain: A - B - C
//                  \
// new chain:        - E - F
//
// a tx that was included in block C might now be in block E or F, and because we store what block every tx is in with its contents, we need to rewrite that
//
// We walk up from the new top header until we see a block that is on the main chain, keeping track of the blocks we see.
// then we send that list to the worker manager, and when we receive the blocks, we regenerate the frontpage response from scratch for the first block,
// and then update it for all blocks that come after.

use anyhow::{Result, bail};
use either::Either;
use rand::Rng;
use slog_scope::info;
use std::{
    collections::{HashMap, VecDeque},
    sync::{LazyLock, OnceLock},
    time::Duration,
};
use tokio::{
    select,
    sync::{broadcast, mpsc, oneshot},
    time::interval,
};

use crate::{
    db::{self, write_analyzed_txs},
    packets::{
        block::{BlockBorrowed, BlockOwned},
        blockheader::{BlockHeaderBorrowed, BlockHeaderOwned, BlockHeaderRef},
        packet::MAX_PACKET_SIZE,
        tx::{TxOutRef, TxRef},
    },
    types::blockmetrics::BlockMetrics,
    util::arena::Arena,
};

use super::{
    CHAIN, generate_frontpage_data, mark_as_downloaded, update_frontpage_data,
    validate_and_apply_header_inner,
};

pub static NEW_HEADER_BROADCAST: LazyLock<(
    broadcast::Sender<[u8; 32]>,
    broadcast::Receiver<[u8; 32]>,
)> = LazyLock::new(|| broadcast::channel(32));

pub static WORKER_REGISTRATION_CHANNEL: OnceLock<mpsc::Sender<oneshot::Sender<[u8; 32]>>> =
    OnceLock::new();

static RECEIVED_HEADER_CHANNEL: OnceLock<mpsc::Sender<BlockHeaderOwned>> = OnceLock::new();
static RECEIVED_BLOCK_CHANNEL: OnceLock<mpsc::Sender<BlockOwned>> = OnceLock::new();

enum FrontPageDataUpdateStrategy {
    RegenerateFromScratch,
    Append,
}

pub fn start() {
    let (worker_tx, worker_rx) = mpsc::channel(16);
    WORKER_REGISTRATION_CHANNEL
        .set(worker_tx)
        .expect("to set worker registration channel");
    let (new_header_tx, new_header_rx) = mpsc::channel(16);
    RECEIVED_HEADER_CHANNEL
        .set(new_header_tx)
        .expect("to set RECEIVED_HEADER_CHANNEL");
    let (new_block_tx, new_block_rx) = mpsc::channel(16);
    RECEIVED_BLOCK_CHANNEL
        .set(new_block_tx)
        .expect("to set RECEIVED_BLOCK_CHANNEL");
    let (job_tx, job_rx) = mpsc::channel(16);
    tokio::spawn(worker_manager(worker_rx, job_rx));
    tokio::spawn(chain_watcher(job_tx, new_header_rx, new_block_rx));
}

pub async fn on_header_received(header: &BlockHeaderBorrowed<'_>) {
    if let Some(c) = RECEIVED_HEADER_CHANNEL.get() {
        let _ = c.send((*header).into()).await;
    }
}

pub async fn on_block_received(block: &BlockBorrowed<'_>) {
    if let Some(c) = RECEIVED_BLOCK_CHANNEL.get() {
        let _ = c.send((*block).into()).await;
    }
}

pub async fn worker_manager(
    mut worker_rx: mpsc::Receiver<oneshot::Sender<[u8; 32]>>,
    mut job_rx: mpsc::Receiver<[u8; 32]>,
) -> Result<()> {
    let mut workers = Vec::with_capacity(1024 * 10);
    loop {
        select! {
            new_worker = worker_rx.recv() => {
                let new_worker = new_worker.unwrap();
                workers.push(new_worker);
            }
            job = job_rx.recv() => {
                let job = match job {
                    Some(j) => j,
                    None => bail!("job channel closed")
                };
                while !workers.is_empty() {
                    let worker_idx = {
                        let mut rng = rand::rng();
                        rng.random_range(0..workers.len())
                    };
                    let worker = workers.swap_remove(worker_idx);
                    // if the worker quit and the job is lost, continue with the loop
                    // if the job was sent successfully, break the loop
                    if worker.send(job).is_ok() {
                        break
                    }
                }
            }
        }
    }
}

async fn chain_watcher(
    job_tx: mpsc::Sender<[u8; 32]>,
    mut new_header_rx: mpsc::Receiver<BlockHeaderOwned>,
    mut new_block_rx: mpsc::Receiver<BlockOwned>,
) -> Result<()> {
    let mut queue: VecDeque<(u64, [u8; 32], FrontPageDataUpdateStrategy)> = VecDeque::new();
    let mut ticker = interval(Duration::from_millis(100));
    loop {
        select! {
            _ = ticker.tick() => {
                if queue.is_empty() {
                    continue
                }
                job_tx.send(queue[0].1).await.expect("to send job to worker manager");
            }
            new_header = new_header_rx.recv() => {
                let new_header = match new_header {
                    Some(v) => v,
                    None => bail!("header tx closed")
                };
                handle_new_header(new_header, &mut queue).await;
            }
            new_block = new_block_rx.recv() => {
                let new_block = match new_block {
                    Some(v) => v,
                    None => bail!("block tx closed")
                };
                handle_new_block(new_block, &mut queue).await;
            }
        }
    }
}

async fn handle_new_header(
    header: BlockHeaderOwned,
    queue: &mut VecDeque<(u64, [u8; 32], FrontPageDataUpdateStrategy)>,
) {
    let c = CHAIN.read().unwrap();
    if c.known_headers.contains_key(&header.hash) {
        return;
    }

    // if we dont know of the header's parent, ignore it
    if !c.known_headers.contains_key(&header.parent) {
        return;
    }

    // from now we'll need write access so we drop the read lock and acquire a write lock
    drop(c);
    let mut w = CHAIN.write().unwrap();

    if let Ok(Some(blocks_to_apply)) =
        validate_and_apply_header_inner(BlockHeaderRef::Owned(&header), &mut w, false)
    {
        let new_top_number = w.top_header.number;
        if blocks_to_apply.len() == 1 {
            // Simple chain extension
            queue.push_back((
                new_top_number,
                header.hash,
                FrontPageDataUpdateStrategy::Append,
            ));
        } else {
            // Fork
            let first_number = new_top_number - blocks_to_apply.len() as u64 - 1;
            for i in 0..blocks_to_apply.len() {
                let hash = blocks_to_apply[i];
                // Regenerate from scratch for the first block
                if i == 0 {
                    queue.push_back((
                        first_number + i as u64,
                        hash,
                        FrontPageDataUpdateStrategy::RegenerateFromScratch,
                    ));
                    info!("chain extended"; "new_top_header" => BlockHeaderRef::Owned(&w.top_header.header).human_hash(), "number" => new_top_number);
                } else {
                    // then append
                    queue.push_back((
                        first_number + i as u64,
                        hash,
                        FrontPageDataUpdateStrategy::Append,
                    ));
                    info!("chain forked"; "new_top_header" => BlockHeaderRef::Owned(&w.top_header.header).human_hash(), "number" => new_top_number, "fork_length" => blocks_to_apply.len());
                }
            }
        }
        let _ = NEW_HEADER_BROADCAST.0.send(w.top_header.header.hash);
    }
}

async fn handle_new_block(
    block: BlockOwned,
    queue: &mut VecDeque<(u64, [u8; 32], FrontPageDataUpdateStrategy)>,
) {
    if queue.is_empty() {
        return;
    }
    if block.header.hash != queue[0].1 {
        return;
    }

    let (number, _, strat) = queue.pop_front().unwrap();
    let hh = BlockHeaderRef::Owned(&block.header).human_hash();
    apply_block(number, block, strat).await;
    info!("block applied"; "hash" => hh, "number" => number);
}

async fn apply_block(number: u64, block: BlockOwned, frontpage_strat: FrontPageDataUpdateStrategy) {
    info!("applying block"; "hash" => BlockHeaderRef::Owned(&block.header).human_hash());
    // first fetch all dependencies
    let mut self_txs = HashMap::with_capacity(block.txs.len());
    for tx in &block.txs {
        self_txs.insert(tx.hash, tx);
    }
    let mut pending_deps = vec![];
    let mut deps_handles = vec![];
    for tx in &block.txs {
        let txref = TxRef::Owned(tx);
        if txref.is_coinbase() {
            continue;
        }
        for txin in &tx.txins {
            if let Some(from) = self_txs.get(&txin.prevout_hash) {
                let txoutref = TxOutRef::Owned(&from.txouts[txin.prevout_index as usize]);
                pending_deps.push(Either::Left(txoutref));
            } else {
                let hash = txin.prevout_hash;
                let idx = txin.prevout_index;
                deps_handles.push(tokio::spawn(async move {
                    let txouts = db::rocksdb::get_transaction_outputs(hash)
                        .await
                        .expect("to have found txouts in the db");

                    txouts[idx as usize].clone()
                }));
                pending_deps.push(Either::Right(deps_handles.len() - 1));
            }
        }
    }

    let mut owned_deps = Vec::with_capacity(deps_handles.len());
    for handle in deps_handles {
        owned_deps.push(handle.await.unwrap());
    }

    let mut deps = Vec::with_capacity(pending_deps.len());

    for dep in pending_deps {
        match dep {
            Either::Left(dep) => {
                deps.push(dep);
            }
            Either::Right(owned_idx) => {
                deps.push(TxOutRef::Owned(&owned_deps[owned_idx]));
            }
        }
    }

    // now that we have all of the dependencies `deps`, analyze all txs
    let analyzed_arena = Arena::new(MAX_PACKET_SIZE);
    let mut analyzed_txs = Vec::with_capacity(block.txs.len());
    let mut consumed = 0;
    let mut fees_total = 0;
    let mut volume = 0;
    let mut fee_rates = Vec::with_capacity(block.txs.len() - 1);
    for tx in &block.txs {
        let txref = TxRef::Owned(tx);
        let txin_count = if txref.is_coinbase() {
            0
        } else {
            tx.txins.len()
        };
        let analyzed = crate::tx::analyze_tx(
            number,
            block.header.hash,
            txref,
            &deps[consumed..consumed + txin_count],
            &analyzed_arena,
        );
        consumed += txin_count;

        if !txref.is_coinbase() {
            let size_vbytes = f64::ceil(analyzed.size_wus as f64 / 4.);
            fee_rates.push(analyzed.fee as f64 / size_vbytes);
        }
        fees_total += analyzed.fee;
        volume += analyzed.txouts_sum;

        analyzed_txs.push(analyzed);
    }

    // calculate block metrics
    let (lowest, highest, median, average) = if fee_rates.is_empty() {
        (0., 0., 0., 0.)
    } else {
        fee_rates.sort_by(|a, b| a.partial_cmp(b).expect("fee rate to be not nan"));

        (
            fee_rates[0],
            fee_rates[fee_rates.len() - 1],
            fee_rates[fee_rates.len() / 2],
            fees_total as f64 / fee_rates.len() as f64,
        )
    };
    let block_metrics = BlockMetrics {
        fees_total,
        volume,
        txs_count: block.txs.len() as u64,
        average_fee_rate: average,
        lowest_fee_rate: lowest,
        highest_fee_rate: highest,
        median_fee_rate: median,
    };

    let bms = [block_metrics];
    write_analyzed_txs(&[block.header.hash], &[&analyzed_txs], &bms).await;
    mark_as_downloaded(vec![block.header.hash]);
    match frontpage_strat {
        FrontPageDataUpdateStrategy::RegenerateFromScratch => {
            generate_frontpage_data(block.header.hash).await;
        }
        FrontPageDataUpdateStrategy::Append => {
            update_frontpage_data(block.header.hash, &bms, &[&analyzed_txs]).await;
        }
    }
}
