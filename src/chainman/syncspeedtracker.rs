use std::{collections::VecDeque, sync::Mutex, time::Instant};

pub struct SyncSpeedTracker {
    inner: Mutex<SyncSpeedTrackerInner>,
}

struct SyncSpeedTrackerInner {
    last_start: Instant,
    last_speeds: VecDeque<f64>, // blocks per second
}

impl SyncSpeedTracker {
    pub fn new(keep_last_ticks: usize) -> Self {
        Self {
            inner: Mutex::new(SyncSpeedTrackerInner {
                last_start: Instant::now(),
                last_speeds: VecDeque::with_capacity(keep_last_ticks + 1),
            }),
        }
    }

    pub fn tick(&self, blocks_received: usize) {
        let mut w = self.inner.lock().unwrap();
        let elapsed_seconds = w.last_start.elapsed().as_secs_f64();
        w.last_start = Instant::now();
        let speed = blocks_received as f64 / elapsed_seconds;
        w.last_speeds.push_back(speed);
        if w.last_speeds.len() == w.last_speeds.capacity() {
            w.last_speeds.pop_front();
        }
    }

    pub fn estimate_speed(&self) -> f64 {
        let w = self.inner.lock().unwrap();
        if w.last_speeds.len() == 0 {
            return 0.;
        }

        let mut total = 0.;
        for speed in &w.last_speeds {
            total += *speed;
        }
        total / w.last_speeds.len() as f64
    }
}
