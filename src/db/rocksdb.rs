use crate::packets::deserialize_array_owned;
use crate::packets::tx::TxOutOwned;
use crate::packets::varint::deserialize_varint;
use crate::tx::AnalyzedTx;
use crate::tx::deserialize_analyzed_tx;
use anyhow::Result;
use anyhow::bail;
use rocksdb::BlockBasedOptions;
use rocksdb::Cache;
use rocksdb::DBRawIteratorWithThreadMode;
use rocksdb::SliceTransform;
use rocksdb::WriteBufferManager;
use rocksdb::{DB, IngestExternalFileOptions, Options};
use serde::Deserialize;
use serde::Serialize;
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

    let buffer_size = {
        let mut raw_value =
            std::env::var("ROCKSDB_BUFFER_SIZE").expect("read ROCKSDB_BUFFER_SIZE env variable");
        raw_value.retain(|v| v.is_numeric());
        raw_value
            .parse::<usize>()
            .expect("to parse ROCKSDB_BUFFER_SIZE into integer")
    };

    let cache = Cache::new_lru_cache(buffer_size);
    open_options.set_write_buffer_manager(
        &WriteBufferManager::new_write_buffer_manager_with_cache(0, true, cache.clone()),
    );

    let mut block_based_options = BlockBasedOptions::default();
    block_based_options.set_block_size(32 * 1024);
    block_based_options.set_block_cache(&cache);
    open_options.set_block_based_table_factory(&block_based_options);

    open_options
});

static PREFIX_DB_OPEN_OPTIONS: LazyLock<Options> = LazyLock::new(|| {
    let mut cloned = OPEN_OPTIONS.clone();
    cloned.set_prefix_extractor(SliceTransform::create_fixed_prefix(32));
    cloned
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

pub static ADDRESSES_DB: LazyLock<DB> = LazyLock::new(|| {
    rocksdb::DB::open(&PREFIX_DB_OPEN_OPTIONS, "bitlens-addresses")
        .expect("to have opened bitlens-addresses db")
});

pub static TXSPENDS_DB: LazyLock<DB> = LazyLock::new(|| {
    rocksdb::DB::open(&PREFIX_DB_OPEN_OPTIONS, "bitlens-txspends")
        .expect("to have opened bitlens-txspends db")
});

static READ_SEMAPHORE: Semaphore = Semaphore::const_new(1024 * 8);

pub async fn setup_rocksdb() {}

#[derive(Copy, Clone, Default)]
pub struct SerializedTx<'a> {
    pub hash: [u8; 32],
    pub analyzed_tx: Option<&'a [u8]>,
    pub tx_outs: Option<&'a [u8]>,
    pub spent_txos: Option<&'a [&'a [u8; 36]]>,
    // tuples (address bytes + tx hash, delta)
    pub address_amends: Option<&'a [(&'a [u8], i64)]>,

    // The following fields arent written to disk (because they're already present in `analyzed_tx`),
    // but are stored for easy access after writing this transaction to update the front page response.
    pub fee: u64,
    pub txouts_sum: u64,
    pub size_wus: u32,
}

#[derive(bincode::Decode, bincode::Encode, Serialize, Deserialize)]
pub struct BlockTxEntry {
    #[serde(serialize_with = "crate::util::serialize_as_hex::serialize_hash_as_hex_reversed")]
    pub hash: [u8; 32],
    pub value: f64,
    pub fee_sats: u64,
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

pub async fn write_txspends(spends: Vec<(&[u8; 36], [u8; 64])>) {
    let mut writer = rocksdb::SstFileWriter::create(&OPEN_OPTIONS);
    writer
        .open("bitlens-txspends-temp.sst")
        .expect("to open bitlens-txspends-temp.sst");

    for pair in spends {
        writer
            .put(pair.0, pair.1)
            .expect("to add tx hashes into sst file");
    }

    writer.finish().expect("to close bitlens-txspends-temp.sst");
    drop(writer);
    TXSPENDS_DB
        .ingest_external_file_opts(&INGEST_OPTIONS, vec!["bitlens-txspends-temp.sst"])
        .expect("to ingest bitlens-txspends-temp.sst");
    TXSPENDS_DB.flush_wal(true).expect("to flush txspends wal");
    TXSPENDS_DB.flush().expect("to flush txspends");
}

pub async fn write_address_amends(address_amends: Vec<(&[u8], [u8; 40])>) {
    let mut writer = rocksdb::SstFileWriter::create(&OPEN_OPTIONS);
    writer
        .open("bitlens-address-amends-temp.sst")
        .expect("to open bitlens-address-amends-temp.sst");

    let mut last = None;
    for pair in address_amends {
        if last.is_some() && last.unwrap() == pair.0 {
            continue;
        }
        writer
            .put(pair.0, pair.1)
            .expect("to add tx hashes into sst file");
        last = Some(pair.0);
    }

    writer
        .finish()
        .expect("to close bitlens-address-amends-temp.sst");
    drop(writer);
    ADDRESSES_DB
        .ingest_external_file_opts(&INGEST_OPTIONS, vec!["bitlens-address-amends-temp.sst"])
        .expect("to ingest bitlens-address-amends-temp.sst");
    ADDRESSES_DB
        .flush_wal(true)
        .expect("to flush address-amends wal");
    ADDRESSES_DB.flush().expect("to flush address-amends");
}

pub async fn get_block_tx_entries(hash: [u8; 32]) -> Result<Vec<BlockTxEntry>> {
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
                let bytes = &data[continue_at..];
                let (value, consumed) =
                    bincode::decode_from_slice(bytes, bincode::config::standard()).unwrap();
                result.push(value);
                continue_at += consumed;
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

pub async fn get_tx_spends(txhash: [u8; 32]) -> Result<Vec<(u32, [u8; 32], [u8; 32])>> {
    let _g = READ_SEMAPHORE
        .acquire()
        .await
        .expect("to have acquire a read permit from the semaphore");

    Ok(tokio::task::spawn_blocking(move || {
        let mut prefixiter: DBRawIteratorWithThreadMode<_> =
            TXSPENDS_DB.prefix_iterator(txhash).into();
        let mut results = Vec::new();
        while let Some((key, value)) = prefixiter.item() {
            assert_eq!(36, key.len());
            assert_eq!(64, value.len());
            let txo_index = {
                let mut i = [0u8; 4];
                i.copy_from_slice(&key[32..36]);
                u32::from_le_bytes(i)
            };
            let tx = {
                let mut t = [0u8; 32];
                t.copy_from_slice(&value[0..32]);
                t
            };
            let block = {
                let mut b = [0u8; 32];
                b.copy_from_slice(&value[32..64]);
                b
            };
            results.push((txo_index, tx, block));
            prefixiter.next();
        }

        results
    })
    .await?)
}

pub async fn get_address_entires(address: Vec<u8>) -> Result<Vec<([u8; 32], [u8; 32], i64)>> {
    let _g = READ_SEMAPHORE
        .acquire()
        .await
        .expect("to have acquire a read permit from the semaphore");

    Ok(tokio::task::spawn_blocking(move || {
        let mut prefixiter: DBRawIteratorWithThreadMode<_> =
            ADDRESSES_DB.prefix_iterator(&address).into();
        let mut results = Vec::new();
        while let Some((key, value)) = prefixiter.item() {
            assert_eq!(address.len() + 32, key.len());
            assert_eq!(40, value.len());
            let tx_hash = {
                let mut i = [0u8; 32];
                i.copy_from_slice(&key[address.len()..]);
                i
            };
            let block_hash = {
                let mut t = [0u8; 32];
                t.copy_from_slice(&value[0..32]);
                t
            };
            let delta = { i64::from_le_bytes(value[32..40].try_into().unwrap()) };
            results.push((tx_hash, block_hash, delta));
            prefixiter.next();
        }

        results
    })
    .await?)
}
