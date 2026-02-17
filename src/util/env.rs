use std::{env::VarError, fmt::Display, str::FromStr};

use slog_scope::info;

pub fn get_env<T: FromStr + Display>(name: &'static str, default: T) -> T
where
    <T as FromStr>::Err: Display,
{
    match std::env::var(name) {
        Ok(v) => match v.parse::<T>() {
            Ok(v) => v,
            Err(e) => {
                info!("failed to parse {name}: {e} defaulting to {default}");
                default
            }
        },
        Err(e) => {
            if let VarError::NotPresent = e {
                info!("{name} is not set, defaulting to {default}");
            } else {
                info!("failed to read {name}: {e} defaulting to {default}");
            }
            default
        }
    }
}
