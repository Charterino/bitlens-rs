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
use rocksdb::ReadOptions;
use rocksdb::SliceTransform;
use rocksdb::WriteBufferManager;
use rocksdb::{DB, IngestExternalFileOptions, Options};
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

static TXSPENDS_DB_OPEN_OPTIONS: LazyLock<Options> = LazyLock::new(|| {
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
    rocksdb::DB::open(&OPEN_OPTIONS, "bitlens-addresses")
        .expect("to have opened bitlens-addresses db")
});

pub static TXSPENDS_DB: LazyLock<DB> = LazyLock::new(|| {
    rocksdb::DB::open(&TXSPENDS_DB_OPEN_OPTIONS, "bitlens-txspends")
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockTxEntry {
    #[serde(serialize_with = "crate::util::serialize_as_hex::serialize_hash_as_hex_reversed")]
    pub hash: [u8; 32],
    pub value_sats: u64,
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

pub async fn write_txouts(txs: &[&SerializedTx<'_>]) {
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

pub async fn write_txs(txs: &[&SerializedTx<'_>]) {
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

pub async fn write_block_txs(blocks_with_txs_pairs: &[([u8; 32], &[u8])]) {
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

pub async fn write_txspends(spends: &[([u8; 36], [u8; 64])]) {
    let mut writer = rocksdb::SstFileWriter::create(&TXSPENDS_DB_OPEN_OPTIONS);
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

pub async fn write_address_amends(address_amends: &[(&[u8], [u8; 40])]) {
    let mut writer = rocksdb::SstFileWriter::create(&OPEN_OPTIONS);
    writer
        .open("bitlens-address-amends-temp.sst")
        .expect("to open bitlens-address-amends-temp.sst");

    for pair in address_amends {
        writer
            .put(pair.0, pair.1)
            .expect("to add tx hashes into sst file");
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
                result.push(BlockTxEntry {
                    hash: bytes[0..32].try_into().unwrap(),
                    value_sats: u64::from_le_bytes(bytes[32..40].try_into().unwrap()),
                    fee_sats: u64::from_le_bytes(bytes[40..48].try_into().unwrap()),
                    size_wus: u32::from_le_bytes(bytes[48..52].try_into().unwrap()),
                });
                continue_at += 52;
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

pub async fn get_address_entires(
    mut address: Vec<u8>,
    from_timestamp: u64,
    to_timestamp: u64,
    limit: usize,
) -> Result<Vec<([u8; 32], [u8; 32], i64)>> {
    let _g = READ_SEMAPHORE
        .acquire()
        .await
        .expect("to have acquire a read permit from the semaphore");

    Ok(tokio::task::spawn_blocking(move || {
        let expected_key_len = address.len() + 8 + 32;

        let mut start = address.clone();
        let inv_to = u64::MAX - to_timestamp;
        start.extend_from_slice(&inv_to.to_be_bytes());
        let inv_from = u64::MAX - from_timestamp;
        address.extend_from_slice(&inv_from.to_be_bytes());
        let end = address;

        let mut read_options = ReadOptions::default();
        read_options.set_iterate_lower_bound(start.clone());
        read_options.set_iterate_upper_bound(end);

        let mut iterator = ADDRESSES_DB.raw_iterator_opt(read_options);

        iterator.seek(start);

        let mut results = Vec::new();
        while let Some((key, value)) = iterator.item() {
            assert_eq!(expected_key_len, key.len());
            assert_eq!(40, value.len());
            let tx_hash = {
                let mut i = [0u8; 32];
                i.copy_from_slice(&key[expected_key_len - 32..]);
                i
            };
            let block_hash = {
                let mut t = [0u8; 32];
                t.copy_from_slice(&value[0..32]);
                t
            };
            let delta = { i64::from_le_bytes(value[32..40].try_into().unwrap()) };
            results.push((tx_hash, block_hash, delta));

            if results.len() == limit {
                break;
            }

            iterator.next();
        }

        results
    })
    .await?)
}

pub async fn get_address_entires_continue(
    mut address: Vec<u8>,
    after_tx: [u8; 32],
    after_timestamp: u64,
    limit: usize,
) -> Result<Vec<([u8; 32], [u8; 32], i64)>> {
    let _g = READ_SEMAPHORE
        .acquire()
        .await
        .expect("to have acquire a read permit from the semaphore");

    Ok(tokio::task::spawn_blocking(move || {
        let expected_key_len = address.len() + 8 + 32;

        let mut start = address.clone();
        let inv_to = u64::MAX - after_timestamp;
        start.extend_from_slice(&inv_to.to_be_bytes());
        start.extend_from_slice(&after_tx);

        let inv_from = u64::MAX;
        address.extend_from_slice(&inv_from.to_be_bytes());
        let end = address;

        let mut read_options = ReadOptions::default();
        read_options.set_iterate_lower_bound(start.clone());
        read_options.set_iterate_upper_bound(end);

        let mut iterator = ADDRESSES_DB.raw_iterator_opt(read_options);

        iterator.seek(start);
        iterator.next();

        let mut results = Vec::new();
        while let Some((key, value)) = iterator.item() {
            assert_eq!(expected_key_len, key.len());
            assert_eq!(40, value.len());
            let tx_hash = {
                let mut i = [0u8; 32];
                i.copy_from_slice(&key[expected_key_len - 32..]);
                i
            };
            let block_hash = {
                let mut t = [0u8; 32];
                t.copy_from_slice(&value[0..32]);
                t
            };
            let delta = { i64::from_le_bytes(value[32..40].try_into().unwrap()) };
            results.push((tx_hash, block_hash, delta));

            if results.len() == limit {
                break;
            }

            iterator.next();
        }

        results
    })
    .await?)
}

pub struct TopAddressData {
    pub transactions: Vec<([u8; 32], [u8; 32], i64)>,
    pub total_txs: usize,
    pub total_spent: u64,
    pub total_received: u64,
    pub first_seen: u64,
}

pub async fn get_top_address_data(
    mut address: Vec<u8>,
    top_count: usize,
) -> Result<TopAddressData> {
    let _g = READ_SEMAPHORE
        .acquire()
        .await
        .expect("to have acquire a read permit from the semaphore");

    Ok(tokio::task::spawn_blocking(move || {
        let address_len = address.len();
        let expected_key_len = address_len + 8 + 32;

        let mut start = address.clone();
        let inv_to = 0u64;
        start.extend_from_slice(&inv_to.to_be_bytes());
        let inv_from = u64::MAX;
        address.extend_from_slice(&inv_from.to_be_bytes());
        let end = address;

        let mut read_options = ReadOptions::default();
        read_options.set_iterate_lower_bound(start.clone());
        read_options.set_iterate_upper_bound(end);

        let mut iterator = ADDRESSES_DB.raw_iterator_opt(read_options);

        iterator.seek(start);

        let mut results = Vec::new();
        let mut total_txs = 0usize;
        let mut last_time = 0u64;
        let mut total_received = 0u64;
        let mut total_sent = 0u64;
        while let Some((key, value)) = iterator.item() {
            assert_eq!(expected_key_len, key.len());
            assert_eq!(40, value.len());
            let tx_hash = {
                let mut i = [0u8; 32];
                i.copy_from_slice(&key[expected_key_len - 32..]);
                i
            };
            let block_hash = {
                let mut t = [0u8; 32];
                t.copy_from_slice(&value[0..32]);
                t
            };
            let delta = { i64::from_le_bytes(value[32..40].try_into().unwrap()) };

            if results.len() < top_count {
                results.push((tx_hash, block_hash, delta));
            }
            total_txs += 1;

            let mut timestamp_bytes = [0u8; 8];
            timestamp_bytes.copy_from_slice(&key[address_len..address_len + 8]);
            let timestamp = u64::MAX - u64::from_be_bytes(timestamp_bytes);
            last_time = timestamp;

            if delta < 0 {
                total_sent += (-delta) as u64;
            } else {
                total_received += delta as u64;
            }

            iterator.next();
        }

        TopAddressData {
            transactions: results,
            total_txs,
            total_spent: total_sent,
            total_received,
            first_seen: last_time,
        }
    })
    .await?)
}
