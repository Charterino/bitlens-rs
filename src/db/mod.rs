use rocksdb::setup_rocksdb;
use sqlite::setup_sqlite;

mod batch;
mod batched;
mod migrations;
pub mod rocksdb;
pub mod sqlite;

pub async fn setup() {
    // sqlite for peers + headers
    setup_sqlite().await;

    // rocksdb for transactions
    setup_rocksdb().await;
}
