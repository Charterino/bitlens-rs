use core::panic;
use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::{Arc, Mutex, RwLock, atomic::Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use slog_scope::{debug, info};
use tokio::{
    join, select,
    sync::mpsc::{Receiver, Sender, channel},
    time::Instant,
};

use crate::{
    addrman,
    db::{self, rocksdb::SerializedTx, write_analyzed_txs},
    ok_or_break, ok_or_continue, ok_or_return,
    packets::{
        block::BlockBorrowed,
        getdata::GetDataOwned,
        invvector::{InventoryVectorOwned, InventoryVectorType},
        packet::{self, MAX_PACKET_SIZE, PayloadWithAllocator},
        packetpayload::{PayloadToSend, ReceivedPayload},
        tx::{TxOutBorrowed, TxOutOwned, TxOutRef, TxRef},
    },
    some_or_return, tx,
    types::blockmetrics::BlockMetrics,
    util::{arena::Arena, speedtracker::TOTAL_IN, timetracker::TimeTracker},
    with_deadline,
};
use anyhow::Result;

use super::{get_block_hashes_to_download, mark_as_downloaded, update_frontpage_response};

const BLOCK_TIMEOUT: Duration = Duration::from_millis(5000);
const INITIAL_WORKER_COUNT: usize = 20;
const MAX_WORKERS: usize = 200;

pub enum DownloadWorkerMessage {
    PullFirstJob(tokio::sync::oneshot::Sender<BlockHashWithNumber>),
    PushResultAndGetNewJob(
        BlockWithNumber,
        tokio::sync::oneshot::Sender<BlockHashWithNumber>,
    ),
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
    let master = tokio::spawn(run_master(master_tx, master_rx, flush_tx));
    let block_writer = tokio::spawn(receive_and_process_blocks(flush_rx));
    let _ = join!(master, block_writer);
    packet::CURRENT_POOL_LIMIT.store(packet::INITIAL_DESERIALIZE_ARENA_COUNT, Ordering::Relaxed);
}

struct MasterState {
    missing_blocks: Vec<[u8; 32]>,
    first_missing_number: u64,
    last_request_times: Vec<u64>,
    next_to_apply: usize,
    next_never_asked_for: usize,
    backlog: Vec<BlockWithNumber>,
    block_avg_size: f64,
    alpha: f64,
    received_since_last_update: usize,
    flush_time_tracker: TimeTracker,
}

async fn run_master(
    master_tx: Sender<DownloadWorkerMessage>,
    mut master_rx: Receiver<DownloadWorkerMessage>,
    flush: Sender<BlockWithNumber>,
) {
    let mut workers = Vec::with_capacity(INITIAL_WORKER_COUNT);
    // Spawn initial workers
    for _ in 0..INITIAL_WORKER_COUNT {
        workers.push(tokio::spawn(run_download_worker(master_tx.clone())));
    }
    let (missing_blocks, first_missing_number) = some_or_return!(get_block_hashes_to_download());
    info!("got missing blocks"; "first" => first_missing_number, "count" => missing_blocks.len());

    let last_request_times = vec![0u64; missing_blocks.len()];
    let mut state = MasterState {
        missing_blocks,
        first_missing_number,
        last_request_times,
        next_to_apply: 0,
        next_never_asked_for: 0,
        // backlog will store blocks we will need later but dont need RIGHT NOW.
        // we need to apply blocks in order, so if we're waiting on #2, but get #3, we store it in the backlog.
        // then when we finally get #2, we can apply #3 right after without waiting.
        // youngest-first, for example (we're waiting on #2): 9, 5, 4, 3
        backlog: Vec::with_capacity(256),
        // Tracks how much time in total we've spent writing the blocks to `flush`
        flush_time_tracker: TimeTracker::new(),
        // Tracks how many blocks we have downloaded since the last update
        received_since_last_update: 0,
        // EWMA of the last blocks
        block_avg_size: 1.,
        // Constant value used for EWMA
        alpha: 2. / (100. + 1.),
    };

    let mut ticker = tokio::time::interval(Duration::from_secs(10));
    // Tracks how many bytes we have downloaded since the last update
    let mut last_in = TOTAL_IN.load(Ordering::Relaxed);
    // Tracks how much time since the last update total we've spent writing the blocks to `flush`
    let mut last_time = Instant::now();
    loop {
        if master_tx.strong_count() == 1 {
            // All workers are dead.
            break;
        }
        select! {
            _ = ticker.tick() => {
                // Update workers

                // First we figure out how many bytes we downloaded, how many blocks, etc
                let current_in = TOTAL_IN.load(Ordering::Relaxed);
                let downloaded_bytes = current_in - last_in;

                let elapsed_time_seconds = ((last_time.elapsed().as_micros() as u64) - state.flush_time_tracker.wait_time_micros()) as f64 / 1000. / 1000.;

                let r = downloaded_bytes as f64 / (state.block_avg_size * workers.len() as f64) / elapsed_time_seconds;
                if r >= 0.9 && workers.len() < MAX_WORKERS {
                    workers.push(tokio::spawn(run_download_worker(master_tx.clone())));
                    debug!("spawned a new block download worker"; "new_count" => workers.len());
                } else if r <= 0.5 && workers.len() > 1 {
                    let a = workers.pop().unwrap();
                    a.abort();
                    debug!("killed a block download worker"; "new_count" => workers.len());
                }

                // At the end update the variables to prepare for the next iteration
                last_in = current_in;
                state.received_since_last_update = 0;
                state.flush_time_tracker.reset();
                last_time = Instant::now();
            }
            message = master_rx.recv() => {
                if message.is_none() {
                    break
                }
                let message = message.unwrap();
                handle_worker_message(message, &mut state, &flush).await;
            }
        }
    }
}

async fn handle_worker_message(
    message: DownloadWorkerMessage,
    state: &mut MasterState,
    flush: &Sender<BlockWithNumber>,
) {
    match message {
        DownloadWorkerMessage::PullFirstJob(sender) => {
            match find_next_best_job(
                state.next_to_apply,
                &mut state.next_never_asked_for,
                &mut state.last_request_times,
                &state.missing_blocks,
                state.first_missing_number,
                true,
            ) {
                Some(job) => {
                    _ = sender.send(job);
                }
                None => {
                    // No job for this runner, kill it
                    drop(sender);
                }
            }
        }
        DownloadWorkerMessage::PushResultAndGetNewJob((payload, number), sender) => {
            state.received_since_last_update += 1;
            state.block_avg_size = (1. - state.alpha) * state.block_avg_size
                + state.alpha * payload.borrow_allocator_with_buffer().borrow_buffer().len() as f64;
            // first either process the payload directly or insert it into backlog
            if number > state.first_missing_number + state.next_to_apply as u64 {
                // This block is ahead of what we're currently waiting for, insert it into the backlog
                if let Err(i) = state.backlog.binary_search_by(|f| number.cmp(&f.1)) {
                    state.backlog.insert(i, (payload, number));
                }
                // Mark this block as fetched so we never ask for it again
                state.last_request_times[(number - state.first_missing_number) as usize] = 0
            } else if number < state.first_missing_number + state.next_to_apply as u64 {
                // We waited for this block for a while and then asked some other peer for it, got it from that peer, and now the first peer is responding. Discard it
            } else {
                // Just the block we need!
                state.next_to_apply += 1;
                state.flush_time_tracker.start();
                ok_or_return!(flush.send((payload, number)).await);
                // Apply the backlog so can_advance is correct
                apply_from_backlog(
                    &mut state.backlog,
                    state.first_missing_number,
                    &mut state.next_to_apply,
                    flush,
                )
                .await;
                state.flush_time_tracker.stop();
            }

            // then respond with the next job
            let can_advance = state.backlog.len() <= 512;
            match find_next_best_job(
                state.next_to_apply,
                &mut state.next_never_asked_for,
                &mut state.last_request_times,
                &state.missing_blocks,
                state.first_missing_number,
                can_advance,
            ) {
                Some(job) => {
                    _ = sender.send(job);
                }
                None => {
                    // No job for this runner, kill it
                    drop(sender);
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

        if elapsed > 1_000 {
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
    let (reply_tx, mut reply_rx) = tokio::sync::oneshot::channel::<BlockHashWithNumber>();
    let (mut job, mut job_number) =
        some_or_return!(fetch_job(&master, reply_tx, &mut reply_rx).await);
    let mut job_packet = job_to_getdata(job);

    // the peer loop: every iteration is a different peer, and "continue" is used to switch to a new peer
    loop {
        let mut connection = addrman::connect_to_good_peer(Some(8)).await;
        // ok_or_continue! will match the result of `write_packet`, and if it's not Result::Ok(), it will `continue`, thus switching to a new peer.
        ok_or_continue!(connection.inner.write_packet(&job_packet).await);
        let mut deadline = Instant::now().checked_add(BLOCK_TIMEOUT).unwrap();
        loop {
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel::<BlockHashWithNumber>();
            // read_packet() returns a Result<Packet>. with_deadline!() will wrap the future and return an error if it doesnt complete before `deadline`.
            // ok_or_break!() will match that and `break` if its Result::Err(), breaking this loop, thus switching to a new peer.
            let packet = ok_or_break!(with_deadline!(connection.inner.read_packet(), deadline));
            // We have a packet from the remote peer we're connected to!
            let should_send = packet.payload.with_payload(|payload| {
                if let Some(ReceivedPayload::Block(b)) = payload {
                    return b.header.hash == job;
                }
                false
            });
            if should_send {
                ok_or_return!(
                    master
                        .send(DownloadWorkerMessage::PushResultAndGetNewJob(
                            (packet.payload, job_number),
                            reply_tx,
                        ))
                        .await
                );
                (job, job_number) = ok_or_return!(reply_rx.await);
                job_packet = job_to_getdata(job);
                ok_or_break!(connection.inner.write_packet(&job_packet).await);
                deadline = Instant::now().checked_add(BLOCK_TIMEOUT).unwrap();
            }
        }
    }
}

async fn fetch_job(
    master: &Sender<DownloadWorkerMessage>,
    send: tokio::sync::oneshot::Sender<BlockHashWithNumber>,
    recv: &mut tokio::sync::oneshot::Receiver<BlockHashWithNumber>,
) -> Option<([u8; 32], u64)> {
    match master.send(DownloadWorkerMessage::PullFirstJob(send)).await {
        Ok(_) => (recv.await).ok(),
        Err(_) => None,
    }
}

fn job_to_getdata(hash: [u8; 32]) -> PayloadToSend {
    PayloadToSend::GetData(GetDataOwned {
        inner: vec![InventoryVectorOwned {
            inv_type: InventoryVectorType::WitnessBlock,
            hash,
        }],
    })
}

const MAX_BLOCKS_PER_FLUSH: usize = 1024;
const ANALYZED_OVERHEAD_PER_BLOCK: usize = 4096 * 24;
type AnalyzedBlock<'a> = &'a [SerializedTx<'a>];
struct ChainSyncState<'current, 'previous> {
    current_txouts: HashMap<[u8; 32], Vec<&'current TxOutBorrowed<'current>>>,
    previous_txouts: HashMap<[u8; 32], Vec<&'previous TxOutBorrowed<'previous>>>,
    current_analyzed: Vec<AnalyzedBlock<'current>>,
    current_metrics: Vec<BlockMetrics>,
}

async fn receive_and_process_blocks(mut recv: Receiver<(PayloadWithAllocator, u64)>) {
    let current_arena_b = Arena::new(
        MAX_BLOCKS_PER_FLUSH * MAX_PACKET_SIZE + MAX_BLOCKS_PER_FLUSH * ANALYZED_OVERHEAD_PER_BLOCK,
    );
    let mut current_arena = &current_arena_b;
    let previous_arena_b = Arena::new(
        MAX_BLOCKS_PER_FLUSH * MAX_PACKET_SIZE + MAX_BLOCKS_PER_FLUSH * ANALYZED_OVERHEAD_PER_BLOCK,
    );
    let mut previous_arena = &previous_arena_b;
    let mut current_state = Arc::new(RwLock::new(ChainSyncState {
        current_txouts: HashMap::with_capacity(MAX_BLOCKS_PER_FLUSH * 4096), // values live in `current_arena`
        previous_txouts: HashMap::with_capacity(MAX_BLOCKS_PER_FLUSH * 4096), // values live in `previous_arena`
        current_analyzed: Vec::with_capacity(MAX_BLOCKS_PER_FLUSH), // values live in `current_arena`
        current_metrics: Vec::with_capacity(MAX_BLOCKS_PER_FLUSH),  // all heap
    }));
    let previous_analyzed = Arc::new(Mutex::new(Some(Vec::with_capacity(MAX_BLOCKS_PER_FLUSH)))); // values live in `previous_arena`
    let previous_metrics = Arc::new(Mutex::new(Some(Vec::with_capacity(MAX_BLOCKS_PER_FLUSH)))); // all heap

    unsafe {
        let mut scope = async_scoped::Scope::create(async_scoped::spawner::use_tokio::Tokio);

        while !recv.is_closed() {
            // Will get the next MAX_BLOCKS_PER_FLUSH blocks and add them to `current_txouts` and `current_analyzed`
            // and return their hashes
            let next_batch =
                get_next_batch_to_flush(&mut recv, current_state.clone(), current_arena).await;

            // wait for the current flush to finish
            scope.collect().await;

            // So everything is flushed now

            // TODO: can prolly optimize a lil here by reusing metrics/analyzed arrays and hashmaps
            let ChainSyncState {
                current_txouts,
                mut previous_txouts,
                current_analyzed,
                current_metrics,
            } = Arc::into_inner(current_state)
                .expect("to have no other arcs for current_state")
                .into_inner()
                .unwrap();

            // Right now we have:
            // current_txouts + current_analyzed -> current_arena
            // previous_txouts + previous_analyzed -> previous_arena

            // First we reset everything previous_ to prepare it for becoming the new current_
            previous_txouts.clear();
            let mut pm = previous_metrics.lock().unwrap().take().unwrap();
            pm.clear();
            let mut pa = previous_analyzed.lock().unwrap().take().unwrap();
            pa.clear();
            previous_arena.reset();

            let pm_clone = previous_metrics.clone();
            let pa_clone = previous_analyzed.clone();
            scope.spawn(async move {
                write_analyzed_txs(&next_batch, &current_analyzed, &current_metrics).await;
                // after we've written blocks and updated fetched_full in sqlite to true,
                // update fetched_full in chainman's live state
                let top = next_batch[next_batch.len() - 1];
                mark_as_downloaded(next_batch);
                update_frontpage_response(
                    top,
                    Some(&current_metrics),
                    Some(&current_analyzed),
                    false,
                )
                .await;
                *pm_clone.lock().unwrap() = Some(current_metrics);
                *pa_clone.lock().unwrap() = Some(current_analyzed);
            });

            // previous_* is stored as the new current_*
            // and current_txouts is stored as the new previous_txouts
            // old current_analyzed and current_metrics are passed into the write task and are returned to as upon completion via the mutex
            current_state = Arc::new(RwLock::new(ChainSyncState {
                current_txouts: previous_txouts,
                previous_txouts: current_txouts,
                current_analyzed: pa,
                current_metrics: pm,
            }));
            (previous_arena, current_arena) = (current_arena, previous_arena);
        }

        // Ensure all futures r finished
        scope.collect().await;
    }
}

async fn get_next_batch_to_flush<'current, 'previous>(
    recv: &mut Receiver<(PayloadWithAllocator, u64)>,
    current_state: Arc<RwLock<ChainSyncState<'current, 'previous>>>,
    current_arena: &'current Arena,
) -> Vec<[u8; 32]> {
    assert_eq!(current_arena.used(), 0);
    let mut current_blocks = Vec::with_capacity(MAX_BLOCKS_PER_FLUSH);
    unsafe {
        let mut scope = async_scoped::Scope::create(async_scoped::spawner::use_tokio::Tokio);
        while let Some(b) = recv.recv().await {
            if b.1 % 1000 == 0 {
                info!("sync progress"; "current_height" => b.1);
            }
            // Have to instantly insert transactions from this block because they might be needed in the next block
            let index = {
                let payload =
                    b.0.borrow_payload()
                        .as_ref()
                        .expect("the payload to not be empty");
                if let ReceivedPayload::Block(block) = payload {
                    current_blocks.push(block.header.hash);
                    let mut w = current_state.write().unwrap();
                    insert_transactions_from_block(block, current_arena, &mut w.current_txouts)
                        .expect("to have inserted txs from the current block");
                    let index = w.current_analyzed.len();
                    w.current_analyzed.push(Default::default());
                    w.current_metrics.push(Default::default());
                    index
                } else {
                    unreachable!()
                }
            };

            scope.spawn(process_block(
                b.0,
                current_state.clone(),
                current_arena,
                index,
            ));

            if current_blocks.len() == MAX_BLOCKS_PER_FLUSH {
                break;
            }
        }
        scope.collect().await;
    }
    current_blocks
}

// Moves all txouts from the block's arena into the current arena
fn insert_transactions_from_block<'arena>(
    block: &BlockBorrowed<'_>,
    arena: &'arena Arena,
    txouts: &mut HashMap<[u8; 32], Vec<&'arena TxOutBorrowed<'arena>>>,
) -> Result<()> {
    for tx in block.txs.iter() {
        let mut cloned = Vec::with_capacity(tx.txouts.len());
        for txout in tx.txouts.iter() {
            let cloned_script = arena.try_alloc_array_copy(txout.script)?;
            let txout = arena.try_alloc(TxOutBorrowed {
                value: txout.value,
                script: cloned_script,
            })?;
            cloned.push(&*txout);
        }
        txouts.insert(tx.hash, cloned);
    }
    Ok(())
}

async fn process_block<'arena>(
    payload: PayloadWithAllocator,
    current_state: Arc<RwLock<ChainSyncState<'arena, '_>>>,
    analyzed_arena: &'arena Arena,
    index_into_analyzed_and_metrics: usize,
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
            let analyzed_txs = analyzed_arena
                .try_alloc_array_fill_copy(block.txs.len(), SerializedTx::default())
                .expect("to have allocated analyzed_txs");

            // keep track of metrics as we go
            let mut fees_total = 0;
            let mut volume = 0;
            let mut fee_rates = Vec::with_capacity(block.txs.len() - 1);

            for (tx_idx, tx) in block.txs.iter().enumerate() {
                // Assemble the dependencies
                let mut deps = Vec::with_capacity(tx.txins.len());
                for txin_idx in 0..tx.txins.len() {
                    let txin = &tx.txins[txin_idx];
                    if *txin.prevout_hash == [0u8; 32] {
                        continue;
                    }
                    // Is it in current_txouts?..
                    if let Some(txouts) = current_txouts.get(txin.prevout_hash) {
                        let p = *txouts.get(txin.prevout_index as usize).unwrap();
                        deps.push(TxOutRef::Borrowed(p));
                        continue;
                    }
                    // Maybe in previous_txouts?..
                    if let Some(txouts) = previous_txouts.get(txin.prevout_hash) {
                        let p = *txouts.get(txin.prevout_index as usize).unwrap();
                        deps.push(TxOutRef::Borrowed(p));
                        continue;
                    }
                    // Must be in deps_from_disk at `current_deps_index`
                    let dep: &TxOutOwned = &deps_from_disk[deps_from_disk_index];
                    deps.push(TxOutRef::Owned(dep));
                    deps_from_disk_index += 1;
                }
                // Analyze this tx
                let analyzed = tx::analyze_tx(TxRef::Borrowed(tx), &deps, analyzed_arena);
                analyzed_txs[tx_idx] = analyzed;
                if tx_idx != 0 {
                    let size_vbytes = f64::ceil(analyzed.size_wus as f64 / 4.);
                    fee_rates.push(analyzed.fee as f64 / size_vbytes);
                }

                fees_total += analyzed.fee;
                volume += analyzed.txouts_sum;
            }
            drop(r);

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

            // insert analyzed_txs into current_state.
            let mut w = current_state.write().unwrap();
            w.current_analyzed[index_into_analyzed_and_metrics] = analyzed_txs;
            w.current_metrics[index_into_analyzed_and_metrics] = BlockMetrics {
                fees_total,
                volume,
                txs_count: block.txs.len() as u64,
                average_fee_rate: average,
                lowest_fee_rate: lowest,
                highest_fee_rate: highest,
                median_fee_rate: median,
            };
        })
        .await;
}

async fn fetch_missing_dependencies(
    block: &BlockBorrowed<'_>,
    current_state: Arc<RwLock<ChainSyncState<'_, '_>>>,
) -> Vec<TxOutOwned> {
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
                if r.current_txouts.contains_key(txin.prevout_hash) {
                    continue;
                }
                // Skip hashes that we have in previous_txouts
                if r.previous_txouts.contains_key(txin.prevout_hash) {
                    continue;
                }
                w.push(None);
                let len = w.len();
                let cloned_deps = loaded_deps.clone();
                let hash = *txin.prevout_hash;
                let index = txin.prevout_index as usize;
                loaded_deps_handles.push(tokio::task::spawn(fetch_dependency_and_insert(
                    hash,
                    index,
                    cloned_deps,
                    len - 1,
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
    deps: Arc<Mutex<Vec<Option<TxOutOwned>>>>,
    deps_index: usize,
) {
    let txouts = db::rocksdb::get_transaction_outputs(txout_hash)
        .await
        .expect("to have found txouts in the db");
    let target = txouts[txout_index].clone();
    let mut w = deps.lock().unwrap();
    w[deps_index] = Some(target);
}
