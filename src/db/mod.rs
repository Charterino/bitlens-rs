
use crate::{
    db::rocksdb::{write_address_amends, write_txspends},
    packets::varint::{VarInt, length_varint, serialize_varint_into_slice},
    types::blockmetrics::BlockMetrics,
    util::{arena::Arena, arenaarray::ArenaArray},
};
use rocksdb::{SerializedTx, setup_rocksdb, write_block_txs, write_txouts, write_txs};
use sqlite::{mark_blocks_as_downloaded, setup_sqlite};

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

pub async fn write_analyzed_txs(
    blocks: &[[u8; 32]],
    txs: &[&[SerializedTx<'_>]],
    coinbase_asciis: &[&[u8]],
    block_metrics: &[BlockMetrics],
    arena: &Arena,
) {
    assert_eq!(blocks.len(), txs.len());
    assert_eq!(blocks.len(), coinbase_asciis.len());

    let serialized_txhashes = {
        let mut blocks_arena_array: ArenaArray<([u8; 32], &[u8])> = arena
            .try_alloc_arenaarray(blocks.len())
            .expect("to allocate serialized_txhashes");

        for (transactions, block_hash) in txs.iter().zip(blocks) {
            let mut offset = length_varint(transactions.len() as VarInt);
            let serialized = arena
                .try_alloc_array_fill_copy(offset + (52 * transactions.len()), 0u8)
                .expect("to allocate serialized_txhashes for block");
            serialize_varint_into_slice(transactions.len() as u64, serialized);

            for transaction in transactions.iter() {
                serialized[offset..offset + 32].copy_from_slice(&transaction.hash);
                serialized[offset + 32..offset + 40]
                    .copy_from_slice(&(transaction.txouts_sum.to_le_bytes()));
                serialized[offset + 40..offset + 48]
                    .copy_from_slice(&(transaction.fee.to_le_bytes()));
                serialized[offset + 48..offset + 52]
                    .copy_from_slice(&(transaction.size_wus.to_le_bytes()));
                offset += 52;
            }

            blocks_arena_array.push((*block_hash, serialized));
        }

        // sort by hash before ingestion
        blocks_arena_array.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        blocks_arena_array.into_arena_array()
    };

    let collapsed: &[&SerializedTx] = {
        let txs_count = {
            let mut count = 0;
            for transactions in txs {
                count += transactions.len();
            }
            count
        };
        let mut result: ArenaArray<'_, &SerializedTx<'_>> = arena
            .try_alloc_arenaarray(txs_count)
            .expect("to allocate collapsed");

        for transactions in txs {
            for transaction in transactions.iter() {
                result.push(transaction);
            }
        }

        // sort by hash before ingestion
        result.sort_unstable_by(|a, b| a.hash.cmp(&b.hash));
        result.into_arena_array()
    };

    let spends = {
        let spends_count = {
            let mut count = 0;
            for transactions in txs {
                for transaction in transactions.iter() {
                    if let Some(i) = transaction.spent_txos {
                        count += i.len();
                    }
                }
            }
            count
        };
        if spends_count == 0 {
            None
        } else {
            let mut result: ArenaArray<'_, ([u8; 36], [u8; 64])> = arena
                .try_alloc_arenaarray(spends_count)
                .expect("to allocate spends");

            for (transactions, block_hash) in txs.iter().zip(blocks) {
                for transaction in transactions.iter() {
                    if let Some(spends) = transaction.spent_txos {
                        let mut value = [0u8; 64];
                        value[0..32].copy_from_slice(&transaction.hash);
                        value[32..64].copy_from_slice(block_hash);
                        for txin in spends {
                            result.push((**txin, value));
                        }
                    }
                }
            }

            // sort by hash before ingestion
            result.sort_unstable_by(|a, b| a.0.cmp(&b.0));
            Some(result.into_arena_array())
        }
    };

    let address_amends = {
        let amends_count = {
            let mut count = 0;
            for transactions in txs {
                for transaction in transactions.iter() {
                    if let Some(i) = transaction.address_amends {
                        count += i.len();
                    }
                }
            }
            count
        };
        if amends_count == 0 {
            None
        } else {
            let mut result: ArenaArray<'_, (&[u8], [u8; 40])> = arena
                .try_alloc_arenaarray(amends_count)
                .expect("to allocate address_amends");

            for (transactions, block_hash) in txs.iter().zip(blocks) {
                for transaction in transactions.iter() {
                    if let Some(amends) = transaction.address_amends {
                        let mut last_key = None;
                        for txin in amends {
                            let mut value = [0u8; 40];
                            value[0..32].copy_from_slice(block_hash);

                            value[32..40].copy_from_slice(&if last_key.is_some()
                                && last_key.unwrap() == txin.0
                            {
                                let (_, prev_key): (_, [u8; 40]) = result.pop();
                                let prev_value =
                                    i64::from_le_bytes(prev_key[32..40].try_into().unwrap());
                                (prev_value + txin.1).to_le_bytes()
                            } else {
                                txin.1.to_le_bytes()
                            });

                            result.push((txin.0, value));
                            last_key = Some(txin.0);
                        }
                    }
                }
            }

            // sort by hash before ingestion
            result.sort_unstable_by(|a, b| a.0.cmp(b.0));
            Some(result.into_arena_array())
        }
    };

    async_scoped::TokioScope::scope_and_block(|s| {
        s.spawn(write_txouts(collapsed));
        s.spawn(write_txs(collapsed));
        s.spawn(write_block_txs(serialized_txhashes));
        if let Some(amends) = address_amends {
            s.spawn(write_address_amends(amends));
        }
        if let Some(spends) = spends {
            s.spawn(write_txspends(spends));
        }
    });

    // after we're done with txs, update fetched_full in sqlite
    mark_blocks_as_downloaded(blocks, coinbase_asciis, block_metrics);
}
