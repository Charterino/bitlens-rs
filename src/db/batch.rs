use std::{mem::swap, time::Duration};

use rusqlite::Params;
use tokio::{
    select,
    sync::mpsc::{Receiver, Sender, channel},
    time::{Instant, sleep_until},
};

use crate::metrics::METRIC_SQLITE_REQUESTS_TIME;

use super::sqlite::CONNECTION;

const BATCH_SIZE: usize = 2048; // batch a lot of inserts/updates together because it doesn't matter if it executes 5 seconds later
const BATCH_TIMEOUT: Duration = Duration::from_secs(5); // Flush the updates at least every 5 seconds

pub trait Request: Send + Sync {
    fn into_params(self) -> impl Params;
}

pub fn start_batcher<T: Request + 'static>(query: &'static str) -> Sender<T> {
    let (sender, receiver) = channel(1);
    tokio::spawn(batch_process_requests(query, receiver));
    sender
}

pub async fn batch_process_requests(
    query: &'static str,
    mut channel: Receiver<impl Request + 'static>,
) {
    ensure_stmt_prepared(query).await;
    let mut pending_requests = Vec::with_capacity(BATCH_SIZE);
    loop {
        let deadline = Instant::now().checked_add(BATCH_TIMEOUT).unwrap();

        let mut should_continue = true;
        while should_continue {
            select! {
                _ = sleep_until(deadline) => {
                    // Deadline reached, attempt to flush right now
                    should_continue = false;
                }
                request = channel.recv() => {
                    let request = request.unwrap(); // These channels will live for the entire duration of the app, so it's safe to unwrap
                    pending_requests.push(request);
                    if pending_requests.len() == BATCH_SIZE {
                        should_continue = false;
                    }
                }
            }
        }
        // Either filled the `requests` or reached the timeout
        if pending_requests.is_empty() {
            continue;
        }

        let mut requests = Vec::with_capacity(BATCH_SIZE);
        swap(&mut requests, &mut pending_requests);
        let count = requests.len();
        // Flush all the requests yay!
        let mut conn = CONNECTION.lock().unwrap();
        let start = Instant::now();
        let tx = conn.transaction().unwrap();
        {
            let mut stmt = tx.prepare_cached(query).unwrap();
            for request in requests {
                stmt.execute(request.into_params())
                    .unwrap_or_else(|_| panic!("to flush batch requests for {}", query));
            }
        }
        tx.commit().unwrap();
        let duration_total = Instant::now().duration_since(start).as_millis() as f64;
        let duration_per_request = duration_total / count as f64;
        for _ in 0..count {
            METRIC_SQLITE_REQUESTS_TIME.observe(duration_per_request);
        }
    }
}

async fn ensure_stmt_prepared(query: &'static str) {
    let conn = CONNECTION.lock().unwrap();
    conn.prepare_cached(query).unwrap(); // ensure this statement is valid
}
