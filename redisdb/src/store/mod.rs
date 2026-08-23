use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crate::zset::ZSet;

/// Number of independent lock domains. Keys hash to exactly one shard, so
/// concurrent traffic on different keys never contends on a global lock.
const SHARDS: usize = 32;

/// Cache-line alignment keeps neighbouring shards off one core's line when
/// different tasks hammer different keys.
#[repr(align(64))]
struct CacheLine<T>(T);

#[derive(Clone)]
pub enum Value {
    Str(String),
    Bytes(Vec<u8>),
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

    /// Bytes view for bit operations (`Str` keys are read through as bytes).
    fn as_bytes(&self) -> Option<&[u8]> {
        match &self.typ {
            Value::Bytes(b) => Some(b),
            Value::Str(s) => Some(s.as_bytes()),
            _ => None,
        }
    }
}

type Map = HashMap<String, Entry>;

fn shard_of(key: &str) -> usize {
    // FNV-1a — cheap, stable, good enough distribution for shard picking.
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
                        // Sweep ONE shard per tick under its exclusive lock:
                        // bounded stall instead of freezing every key.
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

    // ── KV ──

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
                Value::Bytes(b) => (Some(String::from_utf8_lossy(b).to_string()), false),
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

    pub async fn lpush_all(&self, key: String, vals: Vec<String>) {
        let mut map = self.shard(&key).write().unwrap();
        match map.get_mut(&key) {
            Some(e) => {
                if let Value::List(list) = &mut e.typ {
                    let mut list = list.write().unwrap();
                    let old = std::mem::take(&mut *list);
                    let mut nd: VecDeque<String> = vals.into_iter().collect();
                    nd.extend(old);
                    *list = nd;
                }
            }
            None => {
                map.insert(
                    key,
                    Entry {
                        typ: Value::List(Arc::new(RwLock::new(vals.into_iter().collect()))),
                        exp_at: None,
                    },
                );
            }
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

    // ── Bitfield ──

    pub async fn set_bit(&self, key: &str, bit: u32, on: bool) {
        let byte_idx = (bit / 8) as usize;
        let mask = 1u8 << (bit % 8);
        let mut map = self.shard(key).write().unwrap();
        if let Some(b) = bytes_entry(&mut map, key, byte_idx + 1) {
            set_bit_in(&mut b[byte_idx], mask, on);
        }
    }

    pub async fn get_bit(&self, key: &str, bit: u32) -> bool {
        let map = self.shard(key).read().unwrap();
        match map.get(key) {
            Some(e) if !e.expired() => e.as_bytes().is_some_and(|b| test_bit_in(b, bit)),
            _ => false,
        }
    }

    pub async fn bitcount(&self, key: &str) -> i64 {
        let map = self.shard(key).read().unwrap();
        match map.get(key) {
            Some(e) if !e.expired() => e
                .as_bytes()
                .map_or(0, |b| b.iter().map(|x| x.count_ones() as i64).sum()),
            _ => 0,
        }
    }

    pub async fn bitfield_get(&self, key: &str, offset: u32, width: u32) -> u64 {
        let map = self.shard(key).read().unwrap();
        match map.get(key) {
            Some(e) if !e.expired() => e.as_bytes().map_or(0, |b| bits_to_u64(b, offset, width)),
            _ => 0,
        }
    }

    pub async fn bitfield_set(&self, key: &str, offset: u32, width: u32, val: u64) {
        if width == 0 {
            return;
        }
        let last_byte = ((offset + width - 1) / 8) as usize;
        let mut map = self.shard(key).write().unwrap();
        if let Some(b) = bytes_entry(&mut map, key, last_byte + 1) {
            for i in 0..width {
                let bit = offset + i;
                set_bit_in(
                    &mut b[(bit / 8) as usize],
                    1u8 << (bit % 8),
                    (val >> i) & 1 == 1,
                );
            }
        }
    }
}

/// Returns the entry's owned byte buffer (promoting `Str` to `Bytes` or
/// creating the entry), grown to at least `min_len`. `None` for non-bytes types.
fn bytes_entry<'a>(map: &'a mut Map, key: &str, min_len: usize) -> Option<&'a mut Vec<u8>> {
    if !matches!(
        map.get(key),
        Some(Entry {
            typ: Value::Bytes(_),
            ..
        })
    ) {
        match map.get_mut(key) {
            Some(e) => {
                let Value::Str(s) = &mut e.typ else {
                    return None;
                };
                e.typ = Value::Bytes(std::mem::take(s).into_bytes());
            }
            None => {
                map.insert(
                    key.to_string(),
                    Entry {
                        typ: Value::Bytes(Vec::new()),
                        exp_at: None,
                    },
                );
            }
        }
    }
    let Entry {
        typ: Value::Bytes(b),
        ..
    } = map.get_mut(key)?
    else {
        unreachable!("just normalized above")
    };
    if b.len() < min_len {
        b.resize(min_len, 0);
    }
    Some(b)
}

fn set_bit_in(byte: &mut u8, mask: u8, on: bool) {
    if on {
        *byte |= mask;
    } else {
        *byte &= !mask;
    }
}

fn test_bit_in(bytes: &[u8], bit: u32) -> bool {
    let (byte_idx, bit_idx) = ((bit / 8) as usize, bit % 8);
    byte_idx < bytes.len() && (bytes[byte_idx] >> bit_idx) & 1 == 1
}

fn bits_to_u64(bytes: &[u8], offset: u32, width: u32) -> u64 {
    let mut result = 0u64;
    for i in 0..width {
        if test_bit_in(bytes, offset + i) {
            result |= 1 << i;
        }
    }
    result
}
