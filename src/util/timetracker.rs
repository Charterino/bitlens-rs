use std::time::{SystemTime, UNIX_EPOCH};

use num::ToPrimitive;

#[derive(Default)]
pub struct TimeTracker {
    total_waited_micros: u64,
    last_wait_start: u64,
}

impl TimeTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&mut self) {
        if self.last_wait_start != 0 {
            panic!("start() called on a timetracker that's already in active state")
        }
        self.last_wait_start = current_micros();
    }

    pub fn stop(&mut self) {
        if self.last_wait_start == 0 {
            panic!("stop() called on a timetracker that's already in stopped state")
        }
        let rn = current_micros();
        self.total_waited_micros += rn - self.last_wait_start;
        self.last_wait_start = 0;
    }

    pub fn wait_time_micros(&self) -> u64 {
        self.total_waited_micros
    }

    pub fn reset(&mut self) {
        if self.last_wait_start != 0 {
            panic!("reset() called on a timetracker that's in active state")
        }
        self.total_waited_micros = 0;
    }
}

fn current_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros()
        .to_u64()
        .unwrap()
}
