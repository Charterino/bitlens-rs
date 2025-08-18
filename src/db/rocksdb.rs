use crate::packets::deserialize_array_owned;
use crate::packets::tx::TxOutOwned;
use crate::packets::varint::deserialize_varint;
use crate::tx::AnalyzedTx;
use crate::tx::deserialize_analyzed_tx;
use anyhow::Result;
use anyhow::bail;
use rocksdb::BlockBasedOptions;
use rocksdb::Cache;
use rocksdb::WriteBufferManager;
use rocksdb::{DB, IngestExternalFileOptions, Options};
use std::sync::LazyLock;
use tokio::sync::Semaphore;

static OPEN_OPTIONS: LazyLock<Options> = LazyLock::new(|| {
    let mut open_options = rocksdb::Options::default();
    open_options.set_compression_type(rocksdb::DBCompressionType::Zstd);
    open_options.set_bottommost_compression_type(rocksdb::DBCompressionType::Zstd);
    open_options.create_if_missing(true);
    open_options.increase_parallelism(num_cpus::get() as i32);
    open_options.set_max_background_jobs(num_cpus::get() as i32);
    open_options.set_max_subcompactions(num_cpus::get() as u32);
    open_options.set_optimize_filters_for_hits(true);
    open_options.set_max_open_files(16 * 1024);

    open_options.set_use_direct_io_for_flush_and_compaction(true);

    let cache = Cache::new_lru_cache(1024 * 1024 * 1024); // 1gb
    open_options.set_write_buffer_manager(
        &WriteBufferManager::new_write_buffer_manager_with_cache(
            1024 * 1024 * 1024,
            true,
            cache.clone(),
        ),
    );

    let mut block_based_options = BlockBasedOptions::default();
    block_based_options.set_block_size(32 * 1024);
    block_based_options.set_block_cache(&cache);
    open_options.set_block_based_table_factory(&block_based_options);

    open_options
});

static INGEST_OPTIONS: LazyLock<IngestExternalFileOptions> = LazyLock::new(|| {
    let mut ingest_options = IngestExternalFileOptions::default();
    ingest_options.set_move_files(true);
    ingest_options.set_snapshot_consistency(false);
    ingest_options
});

pub static TXS_DB: LazyLock<DB> = LazyLock::new(|| {
    rocksdb::DB::open(&OPEN_OPTIONS, "bitlens-txs").expect("to have opened bitlens-txs db")
});

pub static TXOUTS_DB: LazyLock<DB> = LazyLock::new(|| {
    rocksdb::DB::open(&OPEN_OPTIONS, "bitlens-txouts").expect("to have opened bitlens-txouts db")
});

pub static BLOCKTXS_DB: LazyLock<DB> = LazyLock::new(|| {
    rocksdb::DB::open(&OPEN_OPTIONS, "bitlens-blocktxs")
        .expect("to have opened bitlens-blocktxs db")
});

static READ_SEMAPHORE: Semaphore = Semaphore::const_new(1024 * 8);

pub async fn setup_rocksdb() {}

#[derive(Copy, Clone, Default)]
pub struct SerializedTx<'a> {
    pub hash: [u8; 32],
    pub analyzed_tx: Option<&'a [u8]>,
    pub tx_outs: Option<&'a [u8]>,

    // The following fields arent written to disk (because they're already present in `analyzed_tx`),
    // but are stored for easy access after writing this transaction to update the front page response.
    pub fee: u64,
    pub txouts_sum: u64,
    pub size_wus: u32,
}

// for now this does the regular alloc stuff and returns owned data with 'static lifetime
pub async fn get_transaction_outputs(hash: [u8; 32]) -> Result<Vec<TxOutOwned>> {
    let _g = READ_SEMAPHORE
        .acquire()
        .await
        .expect("to have acquire a read permit from the semaphore");

    tokio::task::spawn_blocking(move || match TXOUTS_DB.get_pinned(hash)? {
        None => bail!("data does not exist"),
        Some(data) => {
            if data.is_empty() {
                bail!("data empty")
            }

            let (result, _) = deserialize_array_owned(&data)?;
            Ok(result)
        }
    })
    .await?
}

pub async fn write_txouts(txs: &Vec<&SerializedTx<'_>>) {
    let mut writer = rocksdb::SstFileWriter::create(&OPEN_OPTIONS);
    writer
        .open("bitlens-txouts-temp.sst")
        .expect("to open bitlens-txouts-temp.sst");
    let mut last_key = [0u8; 32];
    for tx in txs {
        if tx.hash == last_key {
            continue;
        }
        writer
            .put(tx.hash, tx.tx_outs.unwrap())
            .expect("to add txout into sst file");
        last_key = tx.hash;
    }

    writer.finish().expect("to close bitlens-txouts-temp.sst");
    drop(writer);
    TXOUTS_DB
        .ingest_external_file_opts(&INGEST_OPTIONS, vec!["bitlens-txouts-temp.sst"])
        .expect("to ingest bitlens-txouts-temp.sst");
    TXOUTS_DB.flush_wal(true).expect("to flush txouts wal");
    TXOUTS_DB.flush().expect("to flush txouts");
}

pub async fn write_txs(txs: &Vec<&SerializedTx<'_>>) {
    let mut writer = rocksdb::SstFileWriter::create(&OPEN_OPTIONS);
    writer
        .open("bitlens-txs-temp.sst")
        .expect("to open bitlens-txs-temp.sst");
    let mut last_key = [0u8; 32];
    for tx in txs {
        if tx.hash == last_key {
            continue;
        }
        writer
            .put(tx.hash, tx.analyzed_tx.unwrap())
            .expect("to add tx into sst file");
        last_key = tx.hash;
    }

    writer.finish().expect("to close bitlens-txs-temp.sst");
    drop(writer);
    TXS_DB
        .ingest_external_file_opts(&INGEST_OPTIONS, vec!["bitlens-txs-temp.sst"])
        .expect("to ingest bitlens-txs-temp.sst");
    TXS_DB.flush_wal(true).expect("to flush txs wal");
    TXS_DB.flush().expect("to flush txs");
}

pub async fn write_block_txs(blocks_with_txs_pairs: Vec<(&[u8; 32], Vec<u8>)>) {
    let mut writer = rocksdb::SstFileWriter::create(&OPEN_OPTIONS);
    writer
        .open("bitlens-blocktxs-temp.sst")
        .expect("to open bitlens-blocktxs-temp.sst");

    for pair in blocks_with_txs_pairs {
        writer
            .put(pair.0, pair.1)
            .expect("to add tx hashes into sst file");
    }

    writer.finish().expect("to close bitlens-blocktxs-temp.sst");
    drop(writer);
    BLOCKTXS_DB
        .ingest_external_file_opts(&INGEST_OPTIONS, vec!["bitlens-blocktxs-temp.sst"])
        .expect("to ingest bitlens-blocktxs-temp.sst");
    BLOCKTXS_DB.flush_wal(true).expect("to flush blocktxs wal");
    BLOCKTXS_DB.flush().expect("to flush blocktxs");
}

pub async fn get_block_tx_hashes(hash: [u8; 32]) -> Result<Vec<[u8; 32]>> {
    let _g = READ_SEMAPHORE
        .acquire()
        .await
        .expect("to have acquire a read permit from the semaphore");

    tokio::task::spawn_blocking(move || match BLOCKTXS_DB.get_pinned(hash)? {
        None => bail!("data does not exist"),
        Some(data) => {
            if data.is_empty() {
                bail!("data empty")
            }

            let (count, mut continue_at) = deserialize_varint(&data)?;
            let mut result = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let mut tx_hash: [u8; 32] = [0u8; 32];
                tx_hash.copy_from_slice(&data[continue_at..continue_at + 32]);
                continue_at += 32;
                result.push(tx_hash);
            }
            Ok(result)
        }
    })
    .await?
}

pub async fn get_analyzed_tx(hash: [u8; 32]) -> Result<AnalyzedTx> {
    let _g = READ_SEMAPHORE
        .acquire()
        .await
        .expect("to have acquire a read permit from the semaphore");

    tokio::task::spawn_blocking(move || match TXS_DB.get_pinned(hash)? {
        None => bail!("data does not exist"),
        Some(data) => {
            if data.is_empty() {
                bail!("data empty")
            }

            Ok(deserialize_analyzed_tx(&data, hash))
        }
    })
    .await?
}
