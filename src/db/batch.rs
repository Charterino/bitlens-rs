use std::{mem::swap, time::Duration};

use deadpool_sqlite::rusqlite::Params;
use tokio::{
    select,
    sync::mpsc::Receiver,
    time::{Instant, sleep_until},
};

use super::POOL;

const BATCH_SIZE: usize = 2048; // batch a lot of inserts/updates together because it doesn't matter if it executes 5 seconds later
const BATCH_TIMEOUT: Duration = Duration::from_secs(5); // Flush the updates at least every 5 seconds

pub trait Request: Send + Sync {
    fn into_params(self) -> impl Params;
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
        // Flush all the requests yay!
        let pool = POOL.get().await.unwrap();
        pool.interact(move |conn| {
            let tx = conn.transaction().unwrap();
            let mut stmt = tx.prepare_cached(query).unwrap();
            for request in requests {
                stmt.execute(request.into_params()).unwrap();
            }
            drop(stmt);
            tx.commit().unwrap()
        })
        .await
        .unwrap();
    }
}

async fn ensure_stmt_prepared(query: &'static str) {
    let conn = POOL.get().await.unwrap();
    conn.interact(|conn| {
        conn.prepare_cached(query).unwrap(); // ensure this statement is valid
    })
    .await
    .unwrap();
}
