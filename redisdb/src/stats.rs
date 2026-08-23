use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct Stats {
    pub total_connections: AtomicU64,
    pub active_connections: AtomicI64,
    pub total_commands: AtomicU64,
    pub started_at: Instant,
    pub addr: Mutex<String>,
}

impl Stats {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            total_connections: AtomicU64::new(0),
            active_connections: AtomicI64::new(0),
            total_commands: AtomicU64::new(0),
            started_at: Instant::now(),
            addr: Mutex::new(String::new()),
        })
    }

    pub fn inc_conn(&self) {
        self.total_connections.fetch_add(1, Ordering::Relaxed);
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_conn(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn bump(&self) {
        self.total_commands.fetch_add(1, Ordering::Relaxed);
    }
}
