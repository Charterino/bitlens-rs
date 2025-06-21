use std::sync::LazyLock;

use crate::packets::buffer::Buffer;
use crate::packets::packetpayload::SerializableValue;
use crate::packets::tx::TxOut;
use crate::packets::varint::VarInt;
use anyhow::Result;
use anyhow::bail;
use rocksdb::{DB, IngestExternalFileOptions, Options};
use tokio::sync::Semaphore;

static OPEN_OPTIONS: LazyLock<Options> = LazyLock::new(|| {
    let mut open_options = rocksdb::Options::default();
    open_options.set_compression_type(rocksdb::DBCompressionType::Zstd);
    open_options.set_bottommost_compression_type(rocksdb::DBCompressionType::Zstd);
    open_options.create_if_missing(true);
    open_options.increase_parallelism(num_cpus::get() as i32);
    open_options.set_max_background_jobs(num_cpus::get() as i32);
    open_options.set_max_subcompactions(num_cpus::get() as u32);
    open_options
});

static INGEST_OPTIONS: LazyLock<IngestExternalFileOptions> = LazyLock::new(|| {
    let mut ingest_options = IngestExternalFileOptions::default();
    ingest_options.set_move_files(true);
    ingest_options
});

static TXS_DB: LazyLock<DB> = LazyLock::new(|| {
    rocksdb::DB::open(&OPEN_OPTIONS, "bitlens-txs").expect("to have opened bitlens-txs db")
});

static TXOUTS_DB: LazyLock<DB> = LazyLock::new(|| {
    rocksdb::DB::open(&OPEN_OPTIONS, "bitlens-txouts").expect("to have opened bitlens-txouts db")
});

static BLOCKTXS_DB: LazyLock<DB> = LazyLock::new(|| {
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

    // The following fields arent written to disk (because they're already present in of `analyzed_tx`),
    // but are stored for easy access after writing this transaction to update the front page response.
    pub fee: u64,
    pub value: u64,
    pub size: u32,
}

pub async fn get_transaction_outputs<'alloc>(hash: [u8; 32]) -> Result<Vec<TxOut<'static>>> {
    let _g = READ_SEMAPHORE
        .acquire()
        .await
        .expect("to have acquire a read permit from the semaphore");

    match TXOUTS_DB.get_pinned(hash)? {
        None => bail!("data does not exist"),
        Some(data) => {
            if data.is_empty() {
                bail!("data empty")
            }

            let (count, mut continue_at) = VarInt::deserialize(&data)?;
            let mut result = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let (txout, len) = TxOut::deserialize((&data).with_offset(continue_at)?)?;
                continue_at += len;
                result.push(txout);
            }
            Ok(result)
        }
    }
}
