use crate::chainman;
use crate::miners::config::DEFAULT_MINERS_CONFIG;
use crate::miners::config::Miner;
use crate::miners::config::compile_regexes;
use crate::miners::config::ensure_miners_file_exists;
use crate::miners::config::reload_miners_config;
use notify::Config;
use notify::Event;
use notify::Result;
use notify::Watcher;
use notify::event::MetadataKind;
use regex::bytes::Regex;
use serde::Deserialize;
use serde::Serialize;
use slog_scope::info;
use slog_scope::warn;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::RwLock;
use std::sync::mpsc;
use std::thread;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

mod config;

const MINER_PERCENTAGE_OVER_LAST_SECONDS: u64 = 3600 * 24 * 14; // 2 weeks
static UNKNOWN_ID: LazyLock<MinerId> = LazyLock::new(|| Arc::new("unknown".to_owned()));

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinersPageData {
    pub recent_blocks: VecDeque<RecentBlock>,
    pub recent_shares: Vec<(MinerId, String, f64)>, // miner_id, miner_name, share
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentBlock {
    #[serde(serialize_with = "crate::util::serialize_as_hex::serialize_hash_as_hex_reversed")]
    pub hash: [u8; 32],
    pub number: u64,
    pub coinbase_ascii: String,
    pub miner_id: MinerId,
    pub miner_name: String,
    pub timestamp: u64,
}

#[derive(Default, Clone, Debug)]
pub struct Miners {
    pub rules: HashMap<MinerId, Miner>,
    pub rules_regexes: HashMap<MinerId, Vec<Regex>>,
    pub blocks_mined_by_miners: HashMap<MinerId, Vec<[u8; 32]>>, // miner id -> all mined blocks, the vec is oldest to newest
    pub recent_blocks_mined_by_miners_counts: HashMap<MinerId, u64>, // miner id -> recently mined blocks count
    pub block_miners: HashMap<[u8; 32], MinerId>,                    // block hash -> miner id
    pub page_data: MinersPageData,
}

pub type MinerId = Arc<String>;

static MINERS_PAGE_DATA: LazyLock<RwLock<String>> =
    LazyLock::new(|| RwLock::new(serde_json::to_string(&MinersPageData::default()).unwrap()));

static MINER_DATA: LazyLock<RwLock<Miners>> = LazyLock::new(|| {
    RwLock::new(Miners {
        rules: (*DEFAULT_MINERS_CONFIG).clone(),
        rules_regexes: compile_regexes(&DEFAULT_MINERS_CONFIG).unwrap(),
        block_miners: Default::default(),
        blocks_mined_by_miners: Default::default(),
        recent_blocks_mined_by_miners_counts: Default::default(),
        page_data: Default::default(),
    })
});

pub fn start() {
    thread::spawn(start_miner_watcher);
}

fn start_miner_watcher() {
    if let Err(e) = ensure_miners_file_exists() {
        warn!("failed to ensure miners.yml exists"; "error" => e.to_string());
        return;
    }

    let (tx, rx) = mpsc::channel::<Result<Event>>();

    let mut watcher = match notify::PollWatcher::new(tx, Config::default()) {
        Err(e) => {
            warn!("failed to create a file watcher"; "error" => e.to_string());
            return;
        }
        Ok(w) => w,
    };
    if let Err(e) = watcher.watch(Path::new("miners.yml"), notify::RecursiveMode::NonRecursive) {
        warn!("failed to setup miners.yml file watcher"; "error" => e.to_string());
        return;
    }
    reload_miners_config();

    while let Ok(Ok(update)) = rx.recv() {
        if let notify::EventKind::Modify(notify::event::ModifyKind::Metadata(
            MetadataKind::WriteTime,
        )) = update.kind
        {
            reload_miners_config();
        }
    }
}

pub fn get_miner_for_block_and_share(block: [u8; 32]) -> Option<(MinerId, String, f64)> {
    let r = MINER_DATA.read().unwrap();
    if let Some(miner_id) = r.block_miners.get(&block) {
        let mined_recently = r.recent_blocks_mined_by_miners_counts[miner_id];
        let recent_blocks = r.page_data.recent_blocks.len();
        let share = if recent_blocks != 0 {
            mined_recently as f64 / recent_blocks as f64
        } else {
            0.
        };
        let display_name = if miner_id.as_str() == "unknown" {
            "UNKNOWN".to_string()
        } else {
            r.rules[miner_id].display_name.clone()
        };
        return Some((miner_id.clone(), display_name, share));
    }

    None
}

pub fn get_page_data() -> String {
    MINERS_PAGE_DATA.read().unwrap().clone()
}

pub fn callback_after_headers_loaded() {
    let mut w = MINER_DATA.write().unwrap();
    recalculate_full_miner_stats(&mut w);
}

pub fn callback_chain_extended(
    hashes: &[[u8; 32]],
    numbers: &[u64],
    timestamps: &[u64],
    coinbase_asciis: &[&[u8]],
) {
    let mut w_outer = MINER_DATA.write().unwrap();
    let w: &mut Miners = &mut w_outer;

    // first pop all blocks that are no longer "recent"
    let last_timestamp = *timestamps.last().unwrap();
    let recent_after = last_timestamp - MINER_PERCENTAGE_OVER_LAST_SECONDS;
    while let Some(block) = w.page_data.recent_blocks.front() {
        if block.timestamp < recent_after {
            // this block is no longer recent
            let block = w.page_data.recent_blocks.pop_front().unwrap();
            *w.recent_blocks_mined_by_miners_counts
                .get_mut(&block.miner_id)
                .unwrap() -= 1;
        } else {
            break;
        }
    }

    'block: for (((hash, number), timestamp), coinbase_ascii) in hashes
        .iter()
        .zip(numbers)
        .zip(timestamps)
        .zip(coinbase_asciis)
    {
        let is_recent = *timestamp >= recent_after;

        for (id, rules) in &w.rules_regexes {
            for rule in rules {
                if rule.is_match(coinbase_ascii) {
                    let id = id.clone();
                    // `id` mined this block
                    w.blocks_mined_by_miners.get_mut(&id).unwrap().push(*hash);
                    w.block_miners.insert(*hash, id.clone());

                    if is_recent {
                        let display_name = w.rules[&id].display_name.clone();
                        *w.recent_blocks_mined_by_miners_counts.get_mut(&id).unwrap() += 1;
                        w.page_data.recent_blocks.push_back(RecentBlock {
                            hash: *hash,
                            number: *number,
                            coinbase_ascii: String::from_utf8_lossy(coinbase_ascii).to_string(),
                            miner_name: display_name,
                            miner_id: id,
                            timestamp: *timestamp,
                        });
                    }

                    continue 'block;
                }
            }
        }

        // the block matched no known pools, add it to the unknown pool
        w.block_miners.insert(*hash, UNKNOWN_ID.clone());
        w.blocks_mined_by_miners
            .get_mut(&*UNKNOWN_ID)
            .unwrap()
            .push(*hash);

        if is_recent {
            w.page_data.recent_blocks.push_back(RecentBlock {
                hash: *hash,
                number: *number,
                coinbase_ascii: String::from_utf8_lossy(coinbase_ascii).to_string(),
                miner_id: UNKNOWN_ID.clone(),
                miner_name: "UNKNOWN".to_string(),
                timestamp: *timestamp,
            });
            *w.recent_blocks_mined_by_miners_counts
                .get_mut(&*UNKNOWN_ID)
                .unwrap() += 1;
        }
    }

    w.page_data.recent_shares.clear();
    for (miner_id, mined) in w.recent_blocks_mined_by_miners_counts.iter() {
        if *mined != 0 {
            let display_name = if miner_id.as_str() == "unknown" {
                "UNKNOWN".to_string()
            } else {
                w.rules[miner_id].display_name.clone()
            };
            w.page_data.recent_shares.push((
                miner_id.clone(),
                display_name,
                (*mined as f64) / w.page_data.recent_blocks.len() as f64,
            ));
        }
    }

    let mut w2 = MINERS_PAGE_DATA.write().unwrap();
    *w2 = serde_json::to_string(&w.page_data).unwrap();
}

pub fn callback_chain_reorg() {
    let mut w = MINER_DATA.write().unwrap();
    recalculate_full_miner_stats(&mut w);
}

pub fn callback_config_updated(
    new_rules: HashMap<MinerId, Miner>,
    new_regrets: HashMap<MinerId, Vec<Regex>>,
) {
    let mut w = MINER_DATA.write().unwrap();
    w.rules = new_rules;
    w.rules_regexes = new_regrets;
    recalculate_full_miner_stats(&mut w);
    info!("successfully updated miners.yml config..");
}

fn recalculate_full_miner_stats(data: &mut Miners) {
    data.blocks_mined_by_miners.clear();
    data.block_miners.clear();
    data.page_data.recent_blocks.clear();
    data.recent_blocks_mined_by_miners_counts.clear();

    // Insert the "unknown" miner
    data.blocks_mined_by_miners
        .insert(UNKNOWN_ID.clone(), Default::default());
    data.recent_blocks_mined_by_miners_counts
        .insert(UNKNOWN_ID.clone(), 0);

    for id in data.rules.keys() {
        data.blocks_mined_by_miners
            .insert(id.clone(), Default::default());
        data.recent_blocks_mined_by_miners_counts
            .insert(id.clone(), 0);
    }

    let hashes_and_asciis = chainman::get_blocks_with_timestamps_and_coinbase_asciis();

    let last_timestamp = hashes_and_asciis.last().map(|v| v.2).unwrap_or(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    );

    let recent_after = last_timestamp - MINER_PERCENTAGE_OVER_LAST_SECONDS;
    'block: for (hash, number, timestamp, coinbase_ascii) in hashes_and_asciis {
        let is_recent = timestamp >= recent_after;

        for (id, rules) in &data.rules_regexes {
            for rule in rules {
                if rule.is_match(coinbase_ascii.as_bytes()) {
                    // `id` mined this block
                    data.block_miners.insert(hash, id.clone());
                    data.blocks_mined_by_miners.get_mut(id).unwrap().push(hash);

                    if is_recent {
                        let display_name = data.rules[id].display_name.clone();
                        *data
                            .recent_blocks_mined_by_miners_counts
                            .get_mut(id)
                            .unwrap() += 1;
                        data.page_data.recent_blocks.push_back(RecentBlock {
                            hash,
                            number,
                            coinbase_ascii,
                            miner_name: display_name,
                            miner_id: id.clone(),
                            timestamp,
                        });
                    }

                    continue 'block;
                }
            }
        }

        // the block matched no known pools, add it to the unknown pool
        data.block_miners.insert(hash, UNKNOWN_ID.clone());
        data.blocks_mined_by_miners
            .get_mut(&*UNKNOWN_ID)
            .unwrap()
            .push(hash);

        if is_recent {
            data.page_data.recent_blocks.push_back(RecentBlock {
                hash,
                number,
                coinbase_ascii,
                miner_id: UNKNOWN_ID.clone(),
                miner_name: "UNKNOWN".to_string(),
                timestamp,
            });
            *data
                .recent_blocks_mined_by_miners_counts
                .get_mut(&*UNKNOWN_ID)
                .unwrap() += 1;
        }
    }

    data.page_data.recent_shares.clear();
    for (miner_id, mined) in data.recent_blocks_mined_by_miners_counts.iter() {
        if *mined != 0 {
            let display_name = if miner_id.as_str() == "unknown" {
                "UNKNOWN".to_string()
            } else {
                data.rules[miner_id].display_name.clone()
            };
            data.page_data.recent_shares.push((
                miner_id.clone(),
                display_name,
                (*mined as f64) / data.page_data.recent_blocks.len() as f64,
            ));
        }
    }

    let mut w = MINERS_PAGE_DATA.write().unwrap();
    *w = serde_json::to_string(&data.page_data).unwrap();
}
