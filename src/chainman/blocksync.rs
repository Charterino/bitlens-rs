use std::{
    borrow::Cow,
    collections::HashMap,
    num::NonZeroUsize,
    sync::{Arc, Mutex, RwLock, atomic::Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use slog_scope::info;
use tokio::{
    sync::mpsc::{Receiver, Sender, channel},
    time::Instant,
};

use crate::{
    addrman,
    db::{self, rocksdb::SerializedTx},
    ok_or_break, ok_or_continue, ok_or_return,
    packets::{
        block::Block,
        getdata::GetData,
        invvector::{InventoryVector, InventoryVectorType},
        packet::{self, MAX_PACKET_SIZE, PayloadWithAllocator},
        packetpayload::PacketPayloadType,
        tx::TxOut,
        varstr::VarStr,
    },
    some_or_return, tx,
    util::arena::Arena,
    with_deadline,
};
use anyhow::{Result, bail};

use super::get_block_hashes_to_download;

const BLOCK_TIMEOUT: Duration = Duration::from_millis(1000);

pub enum DownloadWorkerMessage {
    PullFirstJob(Sender<BlockHashWithNumber>),
    PushResultAndGetNewJob(BlockWithNumber, Sender<BlockHashWithNumber>),
}

type BlockHashWithNumber = ([u8; 32], u64);
type BlockWithNumber = (PayloadWithAllocator, u64);

pub async fn sync_blocks() {
    // Increase the max number of deserialize arenas while we're syncing blocks
    packet::CURRENT_POOL_LIMIT.store(
        packet::MAX_DESERIALIZE_ARENA_COUNT_DURING_BLOCKSYNC,
        Ordering::Relaxed,
    );
    let (flush_tx, flush_rx) = channel(1);
    let (master_tx, master_rx) = channel(1);
    // Todo: spawn and kill workers dynamically
    for _ in 0..50 {
        tokio::spawn(run_download_worker(master_tx.clone()));
    }
    tokio::spawn(run_master(master_rx, flush_tx));
    tokio::spawn(receive_and_process_blocks(flush_rx));
    packet::CURRENT_POOL_LIMIT.store(packet::INITIAL_DESERIALIZE_ARENA_COUNT, Ordering::Relaxed);
}

async fn run_master(mut master: Receiver<DownloadWorkerMessage>, flush: Sender<BlockWithNumber>) {
    let (missing_blocks, first_missing_number) = some_or_return!(get_block_hashes_to_download());
    info!("got missing blocks"; "first" => first_missing_number, "count" => missing_blocks.len());
    let mut last_request_times = vec![0u64; missing_blocks.len()]; // worst case theres ~1m blocks thats like 8mb

    let mut next_to_apply: usize = 0;
    let mut next_never_asked_for: usize = 0;

    // backlog will store blocks we will need later but dont need RIGHT NOW.
    // we need to apply blocks in order, so if we're waiting on #2, but get #3, we store it in the backlog.
    // then when we finally get #2, we can apply #3 right after without waiting.
    // youngest-first, for example (we're waiting on #2): 9, 5, 4, 3
    let mut backlog: Vec<BlockWithNumber> = Vec::with_capacity(64);

    loop {
        let message = some_or_return!(master.recv().await);
        match message {
            DownloadWorkerMessage::PullFirstJob(sender) => {
                match find_next_best_job(
                    next_to_apply,
                    &mut next_never_asked_for,
                    &mut last_request_times,
                    &missing_blocks,
                    first_missing_number,
                    true,
                ) {
                    Some(job) => {
                        _ = sender.send(job).await;
                    }
                    None => {
                        // No job for this runner, kill it
                        drop(sender);
                    }
                }
            }
            DownloadWorkerMessage::PushResultAndGetNewJob((payload, number), sender) => {
                // first either process the payload directly or insert it into backlog
                if number > first_missing_number + next_to_apply as u64 {
                    // This block is ahead of what we're currently waiting for, insert it into the backlog
                    if let Err(i) = backlog.binary_search_by(|f| number.cmp(&f.1)) {
                        backlog.insert(i, (payload, number));
                    }
                    // Mark this block as fetched so we never ask for it again
                    last_request_times[(number - first_missing_number) as usize] = 0
                } else if number < first_missing_number + next_to_apply as u64 {
                    // We waited for this block for a while and then asked some other peer for it, got it from that peer, and now the first peer is responding. Discard it
                } else {
                    // Just the block we need!
                    next_to_apply += 1;
                    ok_or_return!(flush.send((payload, number)).await);
                    // Apply the backlog so can_advance is correct
                    apply_from_backlog(
                        &mut backlog,
                        first_missing_number,
                        &mut next_to_apply,
                        &flush,
                    )
                    .await;
                }

                // then respond with the next job
                let can_advance = backlog.len() <= 512;
                match find_next_best_job(
                    next_to_apply,
                    &mut next_never_asked_for,
                    &mut last_request_times,
                    &missing_blocks,
                    first_missing_number,
                    can_advance,
                ) {
                    Some(job) => {
                        _ = sender.send(job).await;
                    }
                    None => {
                        // No job for this runner, kill it
                        drop(sender);
                    }
                }
            }
        }
    }
}

async fn apply_from_backlog(
    backlog: &mut Vec<BlockWithNumber>,
    first_missing_number: u64,
    next_to_apply: &mut usize,
    flush: &Sender<BlockWithNumber>,
) {
    while !backlog.is_empty()
        && backlog.last().unwrap().1 == first_missing_number + *next_to_apply as u64
    {
        let popped = backlog.pop().unwrap();
        *next_to_apply += 1;
        ok_or_return!(flush.send(popped).await);
    }
}

/* So imagine the following:
 * next_never_asked_for: 4 --------------------------↓
 * next_to_apply: 2 ----------↓                      ↓
 *                            ↓                      ↓
 * last_request_times: [0, 0, now()-200, now()-4000, 0, 0, 0, ...]
 *
 * we need to return the "best" job to complete
 * the algo is essentially the following
 *
 * for block in [next_to_apply..next_never_asked_for) {
 *   if asked for this block more than 2 seconds ago, return this block and update last_request_times
 * }
 * if (can_advance) {
 *   return next_never_asked_for++
 * } else {
 *   return the block we've been waiting the longest on
 * }
 */
fn find_next_best_job(
    next_to_apply: usize,
    next_never_asked_for: &mut usize,
    last_request_times: &mut [u64],
    missing_blocks: &[[u8; 32]],
    first_missing_number: u64,
    can_advance: bool,
) -> Option<BlockHashWithNumber> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let mut oldest_asked_for_block = 0;
    let mut oldest_asked_for_block_index: Option<NonZeroUsize> = None;

    // Iterate over the blocks we're already waiting on, see if we've been waiting on them for a long time (currently >2 seconds)
    // if so, ask again
    for i in next_to_apply..*next_never_asked_for {
        // this iteration is gonna be v fast, ~200-300 blocks maybe even less
        let requested_at = last_request_times[i];
        if requested_at == 0 {
            // we have this specific block but we're waiting on a block before it.
            // for example we're waiting on block #4, and we get block #9.
            // we would set last_request_times[9] = 0, to indicate that we already have it.
            continue;
        }
        let elapsed = now - requested_at;

        if elapsed > 2_000 {
            last_request_times[i] = now;
            return Some((missing_blocks[i], first_missing_number + i as u64));
        }

        match oldest_asked_for_block_index {
            None => {
                oldest_asked_for_block_index = Some(NonZeroUsize::new(i + 1).unwrap());
                oldest_asked_for_block = elapsed;
            }
            Some(_) => {
                if elapsed > oldest_asked_for_block {
                    oldest_asked_for_block_index = Some(NonZeroUsize::new(i + 1).unwrap());
                    oldest_asked_for_block = elapsed;
                }
            }
        }
    }

    if !can_advance {
        match oldest_asked_for_block_index {
            Some(index_plus_one) => {
                let i = index_plus_one.get() - 1;
                last_request_times[i] = now;
                return Some((missing_blocks[i], first_missing_number + i as u64));
            }
            None => {
                panic!("uhhh {} {}", next_to_apply, *next_never_asked_for)
            }
        }
    }

    if *next_never_asked_for < missing_blocks.len() {
        let index_to_return = *next_never_asked_for;
        last_request_times[index_to_return] = now;
        *next_never_asked_for += 1;
        return Some((
            missing_blocks[index_to_return],
            first_missing_number + index_to_return as u64,
        ));
    }

    // If we have asked for ALL blocks and we're just waiting, ask for them again?
    if next_to_apply < *next_never_asked_for {
        return Some((
            missing_blocks[next_to_apply],
            first_missing_number + next_to_apply as u64,
        ));
    }

    // we're done!
    None
}

async fn run_download_worker(master: Sender<DownloadWorkerMessage>) {
    let (reply_tx, mut reply_rx) = channel::<BlockHashWithNumber>(1);
    let (mut job, mut job_number) =
        some_or_return!(fetch_job(&master, reply_tx.clone(), &mut reply_rx).await);
    let mut job_packet = job_to_getdata(job);

    // the peer loop: every iteration is a different peer, and "continue" is used to switch to a new peer
    loop {
        let mut connection = addrman::connect_to_good_peer(Some(8)).await;
        // ok_or_continue! will match the result of `write_packet`, and if it's not Result::Ok(), it will `continue`, thus switching to a new peer.
        ok_or_continue!(connection.inner.write_packet(&job_packet).await);
        let mut deadline = Instant::now().checked_add(BLOCK_TIMEOUT).unwrap();
        loop {
            // read_packet() returns a Result<Packet>. with_deadline!() will wrap the future and return an error if it doesnt complete before `deadline`.
            // ok_or_break!() will match that and `break` if its Result::Err(), breaking this loop, thus switching to a new peer.
            let packet = ok_or_break!(with_deadline!(connection.inner.read_packet(), deadline));
            // We have a packet from the remote peer we're connected to!
            let should_send = packet.payload.with_payload(|payload| {
                if let Some(PacketPayloadType::Block(b)) = payload {
                    return b.header.hash == job;
                }
                false
            });
            if should_send {
                ok_or_return!(
                    master
                        .send(DownloadWorkerMessage::PushResultAndGetNewJob(
                            (packet.payload, job_number),
                            reply_tx.clone(),
                        ))
                        .await
                );
                (job, job_number) = some_or_return!(reply_rx.recv().await);
                job_packet = job_to_getdata(job);
                ok_or_continue!(connection.inner.write_packet(&job_packet).await);
                deadline = Instant::now().checked_add(BLOCK_TIMEOUT).unwrap();
            }
        }
    }
}

async fn fetch_job(
    master: &Sender<DownloadWorkerMessage>,
    send: Sender<BlockHashWithNumber>,
    recv: &mut Receiver<BlockHashWithNumber>,
) -> Option<([u8; 32], u64)> {
    match master.send(DownloadWorkerMessage::PullFirstJob(send)).await {
        Ok(_) => recv.recv().await,
        Err(_) => None,
    }
}

fn job_to_getdata(hash: [u8; 32]) -> PacketPayloadType<'static> {
    PacketPayloadType::GetData(Cow::Owned(GetData {
        inner: Cow::Owned(vec![Cow::Owned(InventoryVector {
            inv_type: InventoryVectorType::WitnessBlock,
            hash: Cow::Owned(hash),
        })]),
    }))
}

const MAX_BLOCKS_PER_FLUSH: usize = 1024;
type AnalyzedBlock<'a> = &'a [SerializedTx<'a>];
struct ChainSyncState<'arena> {
    current_txouts: HashMap<[u8; 32], Vec<&'arena TxOut<'arena>>>,
    current_analyzed: Vec<AnalyzedBlock<'arena>>,
    previous_txouts: HashMap<[u8; 32], Vec<&'arena TxOut<'arena>>>,
}

async fn receive_and_process_blocks(mut recv: Receiver<(PayloadWithAllocator, u64)>) {
    let mut current_blocks = Vec::with_capacity(MAX_BLOCKS_PER_FLUSH);
    let current_state = Arc::new(RwLock::new(ChainSyncState {
        current_txouts: HashMap::new(),
        previous_txouts: HashMap::new(),
        current_analyzed: Vec::with_capacity(MAX_BLOCKS_PER_FLUSH),
    }));
    let current_arena: Arena = Arena::new(MAX_BLOCKS_PER_FLUSH * MAX_PACKET_SIZE);
    // TODO
    while let Some(b) = recv.recv().await {
        // Have to instantly insert transactions from this block because they might be needed in the next block
        b.0.with_block(|block| {
            current_blocks.push(block.header.hash);
            let mut w = current_state.write().unwrap();
            insert_transactions_from_block(block, &current_arena, &mut w.current_txouts)
                .expect("to have inserted txs from the current block");
        });

        // local.spawn_local(process_block(b.0, current_state.clone(), &current_arena));

        if current_blocks.len() == MAX_BLOCKS_PER_FLUSH {
            // flush
        }
    }
}

fn insert_transactions_from_block<'arena, 'block>(
    block: &Block<'block>,
    arena: &'arena Arena,
    txouts: &mut HashMap<[u8; 32], Vec<&'arena TxOut<'arena>>>,
) -> Result<()> {
    for tx in block.txs.iter() {
        let mut cloned = Vec::with_capacity(tx.txouts.len());
        for txout in tx.txouts.iter() {
            let cloned_script = arena.try_alloc_array_copy(txout.script.inner.as_ref())?;
            let txout = match arena.try_alloc(TxOut {
                value: txout.value,
                script: Cow::Owned(VarStr {
                    inner: Cow::Borrowed(cloned_script),
                }),
            }) {
                Ok(p) => p,
                Err(_) => bail!("oom"),
            };
            cloned.push(&*txout);
        }
        txouts.insert(tx.hash, cloned);
    }
    Ok(())
}

async fn process_block<'arena>(
    payload: PayloadWithAllocator,
    current_state: Arc<RwLock<ChainSyncState<'arena>>>,
    arena: &'static Arena,
) {
    payload
        .with_block_async(async |block| {
            // First we load all dependencies we dont have from disk:
            let deps_from_disk = fetch_missing_dependencies(block, current_state.clone()).await;
            let mut deps_from_disk_index = 0;

            let r = current_state.read().unwrap();
            let current_txouts = &r.current_txouts;
            let previous_txouts = &r.previous_txouts;
            // Then we assemble dependency arrays
            // Every dependency might be
            // - current_txouts: blocks that are still being processed
            // - previous_txouts: last batch of block that is already processed and is being flushed to disk
            // - deps_from_disk
            let analyzed_txs = arena
                .try_alloc_array_fill_copy(block.txs.len(), SerializedTx::default())
                .expect("to have allocated analyzed_txs");
            for tx_idx in 0..block.txs.len() {
                let tx = &block.txs[tx_idx];
                // Assemble the dependencies
                let mut deps = Vec::with_capacity(tx.txins.len());
                for txin_idx in 0..tx.txins.len() {
                    let txin = &tx.txins[txin_idx];
                    if *txin.prevout_hash == [0u8; 32] {
                        continue;
                    }
                    // Is it in current_txouts?..
                    if let Some(txouts) = current_txouts.get(&*txin.prevout_hash) {
                        let p = *txouts.get(txin.prevout_index as usize).unwrap();
                        deps.push(&*p);
                        continue;
                    }
                    // Maybe in previous_txouts?..
                    if let Some(txouts) = previous_txouts.get(&*txin.prevout_hash) {
                        let p = *txouts.get(txin.prevout_index as usize).unwrap();
                        deps.push(&*p);
                        continue;
                    }
                    // Must be in deps_from_disk at `current_deps_index`
                    let dep: &TxOut<'static> = &deps_from_disk[deps_from_disk_index];
                    let c: &TxOut<'arena> = dep.covariant();
                    deps.push(c);
                    deps_from_disk_index += 1;
                }
                // Analyze this tx
                let analyzed = tx::analyze_tx(tx, &deps, arena);
                analyzed_txs[tx_idx] = analyzed;
            }
            drop(r);
            // insert analyzed_txs into current_state.
            let mut w = current_state.write().unwrap();
            w.current_analyzed.push(analyzed_txs);
        })
        .await;
}

async fn fetch_missing_dependencies(
    block: &Block<'_>,
    current_state: Arc<RwLock<ChainSyncState<'_>>>,
) -> Vec<TxOut<'static>> {
    let loaded_deps = Arc::new(Mutex::new(Vec::new()));
    let mut loaded_deps_handles = Vec::new();
    {
        let r = current_state.read().unwrap();
        let mut w = loaded_deps.lock().unwrap();
        for tx in block.txs.iter() {
            for txin_idx in 0..tx.txins.len() {
                let txin = &tx.txins[txin_idx];
                // Skip empty hashes
                if *txin.prevout_hash == [0u8; 32] {
                    continue;
                }
                // Skip hashes that we have in current_txouts
                if r.current_txouts.contains_key(&*txin.prevout_hash) {
                    continue;
                }
                // Skip hashes that we have in previous_txouts
                if r.previous_txouts.contains_key(&*txin.prevout_hash) {
                    continue;
                }
                w.push(None);
                loaded_deps_handles.push(tokio::spawn(fetch_dependency_and_insert(
                    txin.prevout_hash.clone().into_owned(),
                    txin.prevout_index as usize,
                    loaded_deps.clone(),
                    w.len(),
                )));
            }
        }
    }
    // Wait for all tasks to finish
    for h in loaded_deps_handles {
        h.await.expect("the task to finish successfully");
    }
    // Now we know loaded_deps is populated
    let loaded_deps = Arc::try_unwrap(loaded_deps)
        .expect("all other arcs to be dropped")
        .into_inner()
        .expect("to take ownership of loaded_deps");

    // convert Vec<Option<T>> into Vec<T>, unwrapping all options
    loaded_deps
        .into_iter()
        .map(|option| option.expect("for the option to be Some"))
        .collect()
}

async fn fetch_dependency_and_insert(
    txout_hash: [u8; 32],
    txout_index: usize,
    deps: Arc<Mutex<Vec<Option<TxOut<'static>>>>>,
    deps_index: usize,
) {
    let txouts = db::rocksdb::get_transaction_outputs(txout_hash)
        .await
        .expect("to have found txouts in the db");
    let target = txouts[txout_index].clone();
    let mut w = deps.lock().unwrap();
    w[deps_index] = Some(target);
}
