use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Write},
    path::Path,
    sync::{Arc, LazyLock},
};

use regex::bytes::Regex;
use serde::{Deserialize, Serialize};
use slog_scope::{info, warn};

use crate::miners::{MinerId, callback_config_updated};

pub static DEFAULT_MINERS_CONFIG: LazyLock<HashMap<MinerId, Miner>> = LazyLock::new(|| {
    let mut h = HashMap::new();
    h.insert(
        Arc::new("binance".to_owned()),
        Miner {
            display_name: "Binance".to_string(),
            icon: None,
            regexes: vec!["binance".to_string()],
        },
    );
    h
});

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Miner {
    pub display_name: String,
    pub icon: Option<String>,
    pub regexes: Vec<String>,
}

pub fn ensure_miners_file_exists() -> anyhow::Result<()> {
    let p = Path::new("miners.yml");
    if p.exists() {
        return Ok(());
    }

    let mut file = File::create(p)?;

    let default_serialized = serde_yaml::to_string(&*DEFAULT_MINERS_CONFIG).unwrap();

    file.write_all(default_serialized.as_bytes())?;
    Ok(())
}

pub fn reload_miners_config() {
    info!("reloading miners.yml config..");
    let mut file = match File::open(Path::new("miners.yml")) {
        Err(e) => {
            warn!("failed to open miners.yml"; "error" => e.to_string());
            return;
        }
        Ok(v) => v,
    };

    let mut data = Vec::new();
    if let Err(e) = file.read_to_end(&mut data) {
        warn!("failed to read miners.yml"; "error" => e.to_string());
        return;
    }

    let parsed = match serde_yaml::from_slice::<HashMap<MinerId, Miner>>(&data) {
        Err(e) => {
            warn!("failed to parse miners.yml"; "error" => e.to_string());
            return;
        }
        Ok(v) => v,
    };

    let recompiled_regrets = match compile_regexes(&parsed) {
        Err(e) => {
            warn!("failed to compile regex rules for miners.yml"; "error" => e.to_string());
            return;
        }
        Ok(r) => r,
    };

    callback_config_updated(parsed, recompiled_regrets);
}

pub fn compile_regexes(
    rules: &HashMap<MinerId, Miner>,
) -> anyhow::Result<HashMap<MinerId, Vec<Regex>>> {
    let mut result = HashMap::with_capacity(rules.len());
    for (id, miner) in rules {
        let mut compiled_regrets = Vec::with_capacity(miner.regexes.len());

        for regex in &miner.regexes {
            let insensetive_regex = "(?i)".to_string() + regex;
            let compiled_regex = Regex::new(&insensetive_regex)?;
            compiled_regrets.push(compiled_regex);
        }

        result.insert(id.clone(), compiled_regrets);
    }

    Ok(result)
}
