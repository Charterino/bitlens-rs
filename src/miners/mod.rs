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
use slog_scope::info;
use slog_scope::warn;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::LazyLock;
use std::sync::RwLock;
use std::sync::mpsc;
use std::thread;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

mod config;

const MINER_PERCENTAGE_OVER_LAST_SECONDS: u64 = 3600 * 24 * 14; // 2 weeks

// this could be optimized by storing pointers to miner ids instead of full ids everywhere but
// at this point it's not worth it
#[derive(Default, Clone, Debug)]
pub struct Miners {
    pub rules: HashMap<String, Miner>,
    pub rules_regexes: HashMap<String, Vec<Regex>>,
    pub blocks_mined_by_miners: HashMap<String, Vec<[u8; 32]>>, // miner id -> all mined blocks, the vec is oldest to newest
    pub recent_blocks_mined_by_miners_counts: HashMap<String, usize>, // miner id -> recently mined blocks count
    pub block_miners: HashMap<[u8; 32], String>,                      // block hash -> miner id
    pub recent_blocks: VecDeque<(u64, String)>,                       // (block timestamp, miner id)
}

static MINER_DATA: LazyLock<RwLock<Miners>> = LazyLock::new(|| {
    RwLock::new(Miners {
        rules: (*DEFAULT_MINERS_CONFIG).clone(),
        rules_regexes: compile_regexes(&DEFAULT_MINERS_CONFIG).unwrap(),
        block_miners: Default::default(),
        blocks_mined_by_miners: Default::default(),
        recent_blocks_mined_by_miners_counts: Default::default(),
        recent_blocks: Default::default(),
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

pub fn get_miner_for_block_and_share(block: [u8; 32]) -> Option<(String, String, f64)> {
    let r = MINER_DATA.read().unwrap();
    if let Some(miner_id) = r.block_miners.get(&block) {
        let mined_recently = r.recent_blocks_mined_by_miners_counts[miner_id];
        let recent_blocks = r.recent_blocks.len();
        let share = if recent_blocks != 0 {
            mined_recently as f64 / recent_blocks as f64
        } else {
            0.
        };
        let miner_name = r.rules[miner_id].display_name.clone();
        return Some((miner_id.clone(), miner_name, share));
    }

    None
}

pub fn callback_after_headers_loaded() {
    let mut w = MINER_DATA.write().unwrap();
    recalculate_full_miner_stats(&mut w);
}

pub fn callback_chain_extended(hashes: &[[u8; 32]], timestamps: &[u64], coinbase_asciis: &[&[u8]]) {
    let mut w = MINER_DATA.write().unwrap();

    // first pop all blocks that are no longer "recent"
    let current_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let recent_after = current_timestamp - MINER_PERCENTAGE_OVER_LAST_SECONDS;
    while let Some((timestamp, _)) = w.recent_blocks.front() {
        if *timestamp < recent_after {
            // this block is no longer recent
            let (_, miner_id) = w.recent_blocks.pop_front().unwrap();
            *w.recent_blocks_mined_by_miners_counts
                .get_mut(&miner_id)
                .unwrap() -= 1;
        } else {
            break;
        }
    }

    'block: for ((hash, timestamp), coinbase_ascii) in
        hashes.iter().zip(timestamps).zip(coinbase_asciis)
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
                        *w.recent_blocks_mined_by_miners_counts.get_mut(&id).unwrap() += 1;
                        w.recent_blocks.push_back((*timestamp, id));
                    }

                    continue 'block;
                }
            }
        }

        // the block matched no known pools, add it to the unknown pool
        w.block_miners.insert(*hash, "unknown".to_string());
        w.blocks_mined_by_miners
            .get_mut("unknown")
            .unwrap()
            .push(*hash);

        if is_recent {
            w.recent_blocks
                .push_back((*timestamp, "unknown".to_string()));
            *w.recent_blocks_mined_by_miners_counts
                .get_mut("unknown")
                .unwrap() += 1;
        }
    }
}

pub fn callback_chain_reorg() {
    let mut w = MINER_DATA.write().unwrap();
    recalculate_full_miner_stats(&mut w);
}

pub fn callback_config_updated(
    new_rules: HashMap<String, Miner>,
    new_regrets: HashMap<String, Vec<Regex>>,
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
    data.recent_blocks.clear();
    data.recent_blocks_mined_by_miners_counts.clear();

    // Insert the "unknown" miner
    data.blocks_mined_by_miners
        .insert("unknown".to_string(), Default::default());
    data.recent_blocks_mined_by_miners_counts
        .insert("unknown".to_string(), 0);

    for id in data.rules.keys() {
        data.blocks_mined_by_miners
            .insert(id.clone(), Default::default());
        data.recent_blocks_mined_by_miners_counts
            .insert(id.clone(), 0);
    }

    let current_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let recent_after = current_timestamp - MINER_PERCENTAGE_OVER_LAST_SECONDS;
    let hashes_and_asciis = chainman::get_block_hashes_with_timestamps_and_coinbase_asciis();
    'block: for (hash, timestamp, coinbase_ascii) in hashes_and_asciis {
        let is_recent = timestamp >= recent_after;

        for (id, rules) in &data.rules_regexes {
            for rule in rules {
                if rule.is_match(coinbase_ascii.as_bytes()) {
                    // `id` mined this block
                    data.block_miners.insert(hash, id.clone());
                    data.blocks_mined_by_miners.get_mut(id).unwrap().push(hash);

                    if is_recent {
                        data.recent_blocks.push_back((timestamp, id.to_string()));
                        *data
                            .recent_blocks_mined_by_miners_counts
                            .get_mut(id)
                            .unwrap() += 1;
                    }

                    continue 'block;
                }
            }
        }

        // the block matched no known pools, add it to the unknown pool
        data.block_miners.insert(hash, "unknown".to_string());
        data.blocks_mined_by_miners
            .get_mut("unknown")
            .unwrap()
            .push(hash);

        if is_recent {
            data.recent_blocks
                .push_back((timestamp, "unknown".to_string()));
            *data
                .recent_blocks_mined_by_miners_counts
                .get_mut("unknown")
                .unwrap() += 1;
        }
    }
}
