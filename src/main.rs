//#![allow(async_fn_in_trait, unused_variables)]

use std::sync::mpsc::{Receiver, channel};

use anyhow::Result;
use tokio::{fs::File, io::AsyncWriteExt};

use pprof::protos::Message;

pub mod addrman;
pub mod connect;
pub mod crawler;
pub mod db;
pub mod packets;
pub mod types;

fn ctrl_channel() -> Result<Receiver<()>, ctrlc::Error> {
    let (sender, receiver) = channel();
    ctrlc::set_handler(move || {
        sender.send(()).unwrap();
    })?;

    Ok(receiver)
}

#[tokio::main]
async fn main() -> Result<()> {
    let ctrl_c_events = ctrl_channel()?;

    let guard = pprof::ProfilerGuardBuilder::default()
        .frequency(1000)
        .blocklist(&["libc", "libgcc", "pthread", "vdso"])
        .build()
        .unwrap();
    db::setup().await;
    addrman::start().await;
    let c = tokio::spawn(crawler::crawl_forever());

    println!("waiting");
    ctrl_c_events.recv().unwrap();

    println!("received a ctrl+c");

    c.abort();

    let report = guard.report().build()?;
    let mut file = File::create("profile.pb").await.unwrap();
    let profile = report.pprof().unwrap();

    let mut content = Vec::new();
    profile.write_to_vec(&mut content).unwrap();

    file.write_all(&content).await.unwrap();

    Ok(())
}
