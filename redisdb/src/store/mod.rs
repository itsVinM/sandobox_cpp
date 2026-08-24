use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crate::zset::ZSet;

const SHARDS: usize = 32;

#[repr(align(64))]
struct CacheLine<T>(T);

#[derive(Clone)]
pub enum Value {
    Str(String),
    ZSet(Arc<RwLock<ZSet>>),
    List(Arc<RwLock<VecDeque<String>>>),
}

struct Entry {
    typ: Value,
    exp_at: Option<Instant>,
}

impl Entry {
    fn expired(&self) -> bool {
        self.exp_at.is_some_and(|t| Instant::now() >= t)
    }
}

type Map = HashMap<String, Entry>;

fn shard_of(key: &str) -> usize {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in key.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    (h % SHARDS as u64) as usize
}

#[derive(Clone)]
pub struct Store {
    shards: Arc<Vec<CacheLine<RwLock<Map>>>>,
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

impl Store {
    pub fn new() -> Self {
        Store {
            shards: Arc::new(
                (0..SHARDS)
                    .map(|_| CacheLine(RwLock::new(HashMap::new())))
                    .collect(),
            ),
        }
    }

    pub fn new_with_expiry() -> (Self, tokio::sync::watch::Sender<()>) {
        let store = Self::new();
        let (tx, mut rx) = tokio::sync::watch::channel(());
        let shards = store.shards.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(50));
            let mut cursor = 0usize;
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let mut map = shards[cursor % SHARDS].0.write().unwrap();
                        let now = Instant::now();
                        map.retain(|_, e| e.exp_at.is_none_or(|t| now < t));
                        drop(map);
                        cursor = cursor.wrapping_add(1);
                    }
                    _ = rx.changed() => break,
                }
            }
        });
        (store, tx)
    }

    fn shard(&self, key: &str) -> &RwLock<Map> {
        &self.shards[shard_of(key)].0
    }

    pub async fn len(&self) -> usize {
        let mut n = 0;
        for s in self.shards.iter() {
            n += s.0.read().unwrap().len();
        }
        n
    }

    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    pub async fn get_str(&self, key: &str) -> (Option<String>, bool) {
        let map = self.shard(key).read().unwrap();
        match map.get(key) {
            Some(e) if !e.expired() => match &e.typ {
                Value::Str(s) => (Some(s.clone()), false),
                _ => (None, true),
            },
            _ => (None, false),
        }
    }

    pub async fn set_str(&self, key: String, val: String) {
        self.shard(&key).write().unwrap().insert(
            key,
            Entry {
                typ: Value::Str(val),
                exp_at: None,
            },
        );
    }

    pub async fn del(&self, key: &str) -> bool {
        self.shard(key).write().unwrap().remove(key).is_some()
    }

    pub async fn keys(&self) -> Vec<String> {
        let now = Instant::now();
        let mut out = Vec::new();
        for s in self.shards.iter() {
            let map = s.0.read().unwrap();
            out.extend(
                map.iter()
                    .filter(|(_, e)| e.exp_at.is_none_or(|t| now < t))
                    .map(|(k, _)| k.clone()),
            );
        }
        out
    }

    pub async fn expire(&self, key: &str, ms: i64) -> bool {
        if let Some(e) = self.shard(key).write().unwrap().get_mut(key) {
            e.exp_at = Some(Instant::now() + Duration::from_millis(ms as u64));
            true
        } else {
            false
        }
    }

    pub async fn ttl_ms(&self, key: &str) -> i64 {
        match self.shard(key).read().unwrap().get(key) {
            Some(e) => match e.exp_at {
                Some(t) => t.saturating_duration_since(Instant::now()).as_millis() as i64,
                None => -1,
            },
            None => -2,
        }
    }

    // ── ZSet ──

    pub async fn z_add(&self, key: String, name: String, score: f64) -> (bool, bool) {
        let mut map = self.shard(&key).write().unwrap();
        match map.get_mut(&key) {
            None => {
                let mut z = ZSet::new();
                z.add(name, score);
                map.insert(
                    key,
                    Entry {
                        typ: Value::ZSet(Arc::new(RwLock::new(z))),
                        exp_at: None,
                    },
                );
                (true, false)
            }
            Some(e) => match &e.typ {
                Value::ZSet(zs) => (zs.write().unwrap().add(name, score), false),
                _ => (false, true),
            },
        }
    }

    pub async fn z_rem(&self, key: &str, name: &str) -> (bool, bool) {
        let mut map = self.shard(key).write().unwrap();
        match map.get_mut(key) {
            Some(e) => match &e.typ {
                Value::ZSet(zs) => (zs.write().unwrap().remove(name), false),
                _ => (false, true),
            },
            None => (false, false),
        }
    }

    pub async fn z_score(&self, key: &str, name: &str) -> (Option<f64>, bool) {
        let map = self.shard(key).read().unwrap();
        match map.get(key) {
            Some(e) => match &e.typ {
                Value::ZSet(zs) => (zs.read().unwrap().score(name), false),
                _ => (None, true),
            },
            None => (None, false),
        }
    }

    pub async fn z_query(
        &self,
        key: &str,
        min_score: f64,
        min_name: &str,
        offset: i64,
        limit: i64,
    ) -> (Vec<crate::zset::Entry>, bool) {
        let map = self.shard(key).read().unwrap();
        match map.get(key) {
            Some(e) => match &e.typ {
                Value::ZSet(zs) => (
                    zs.read().unwrap().query(min_score, min_name, offset, limit),
                    false,
                ),
                _ => (vec![], true),
            },
            None => (vec![], false),
        }
    }

    // ── List ──

    pub async fn lpush(&self, key: String, val: String) {
        let mut map = self.shard(&key).write().unwrap();
        match map.get_mut(&key) {
            Some(e) => {
                if let Value::List(list) = &e.typ {
                    list.write().unwrap().push_front(val);
                }
            }
            None => {
                map.insert(
                    key,
                    Entry {
                        typ: Value::List(Arc::new(RwLock::new(VecDeque::from([val])))),
                        exp_at: None,
                    },
                );
            }
        }
    }

    pub async fn rpop(&self, key: &str) -> Option<String> {
        let mut map = self.shard(key).write().unwrap();
        match map.get_mut(key) {
            Some(e) => match &e.typ {
                Value::List(list) => list.write().unwrap().pop_back(),
                _ => None,
            },
            None => None,
        }
    }

    pub async fn lpop(&self, key: &str) -> Option<String> {
        let mut map = self.shard(key).write().unwrap();
        match map.get_mut(key) {
            Some(e) => match &e.typ {
                Value::List(list) => list.write().unwrap().pop_front(),
                _ => None,
            },
            None => None,
        }
    }

    pub async fn lrange(&self, key: &str, start: i64, stop: i64) -> Vec<String> {
        let map = self.shard(key).read().unwrap();
        match map.get(key) {
            Some(e) => match &e.typ {
                Value::List(list) => {
                    let list = list.read().unwrap();
                    let len = list.len() as i64;
                    let s = if start < 0 {
                        (len + start).max(0)
                    } else {
                        start
                    } as usize;
                    let stop = if stop < 0 {
                        (len + stop + 1).max(0)
                    } else {
                        (stop + 1).min(len)
                    } as usize;
                    if s >= stop {
                        vec![]
                    } else {
                        list.iter().skip(s).take(stop - s).cloned().collect()
                    }
                }
                _ => vec![],
            },
            None => vec![],
        }
    }

    pub async fn lindex(&self, key: &str, idx: i64) -> Option<String> {
        let map = self.shard(key).read().unwrap();
        match map.get(key) {
            Some(e) => match &e.typ {
                Value::List(list) => {
                    let list = list.read().unwrap();
                    let len = list.len() as i64;
                    let i = if idx < 0 { len + idx } else { idx };
                    (i >= 0 && i < len)
                        .then(|| list.get(i as usize).cloned())
                        .flatten()
                }
                _ => None,
            },
            None => None,
        }
    }

    pub async fn llen(&self, key: &str) -> i64 {
        let map = self.shard(key).read().unwrap();
        match map.get(key) {
            Some(e) => match &e.typ {
                Value::List(list) => list.read().unwrap().len() as i64,
                _ => -1,
            },
            None => 0,
        }
    }

    pub async fn lrem(&self, key: &str, count: i64, val: &str) -> i64 {
        let mut map = self.shard(key).write().unwrap();
        match map.get_mut(key) {
            Some(e) => match &e.typ {
                Value::List(list) => {
                    let mut list = list.write().unwrap();
                    if count == 0 {
                        let before = list.len();
                        list.retain(|v| v != val);
                        (before - list.len()) as i64
                    } else if count > 0 {
                        let mut removed = 0i64;
                        list.retain(|v| {
                            let drop_it = v == val && removed < count;
                            if drop_it {
                                removed += 1;
                            }
                            !drop_it
                        });
                        removed
                    } else {
                        let abs_count = (-count) as usize;
                        let mut removed = 0usize;
                        let mut i = list.len();
                        while i > 0 && removed < abs_count {
                            i -= 1;
                            if list[i] == val {
                                list.remove(i);
                                removed += 1;
                            }
                        }
                        removed as i64
                    }
                }
                _ => 0,
            },
            None => 0,
        }
    }
}
