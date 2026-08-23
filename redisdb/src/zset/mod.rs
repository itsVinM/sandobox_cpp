use std::collections::{BTreeMap, HashMap};

/// Total order over f64 (scores are never NaN — `add` rejects them, so a
/// manual `Eq` is sound even though f64 alone isn't).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Score(f64);

impl Eq for Score {}

impl Ord for Score {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

impl PartialOrd for Score {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub score: f64,
}

/// Sorted-set: O(log n) add/remove via one BTreeMap ordered by (score, name),
/// plus a HashMap for O(1) score lookups by member name.
pub struct ZSet {
    scores: HashMap<String, f64>,
    ranked: BTreeMap<(Score, String), ()>,
}

impl Default for ZSet {
    fn default() -> Self {
        Self::new()
    }
}

impl ZSet {
    pub fn new() -> Self {
        ZSet {
            scores: HashMap::new(),
            ranked: BTreeMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.scores.len()
    }

    pub fn is_empty(&self) -> bool {
        self.scores.is_empty()
    }

    /// Returns true if the member's score was inserted or changed.
    pub fn add(&mut self, name: String, score: f64) -> bool {
        if score.is_nan() {
            return false;
        }
        if let Some(&old) = self.scores.get(&name) {
            if old == score {
                return false;
            }
            self.ranked.remove(&(Score(old), name.clone()));
        }
        self.scores.insert(name.clone(), score);
        self.ranked.insert((Score(score), name), ());
        true
    }

    pub fn remove(&mut self, name: &str) -> bool {
        match self.scores.remove(name) {
            Some(score) => self
                .ranked
                .remove(&(Score(score), name.to_string()))
                .is_some(),
            None => false,
        }
    }

    pub fn score(&self, name: &str) -> Option<f64> {
        self.scores.get(name).copied()
    }

    /// Entries with (score, name) >= (min_score, min_name), skipping `offset`,
    /// returning at most `limit`.
    pub fn query(&self, min_score: f64, min_name: &str, offset: i64, limit: i64) -> Vec<Entry> {
        if limit <= 0 {
            return Vec::new();
        }
        let start_key = (Score(min_score), min_name.to_string());
        let to_entry = |(s, n): &(Score, String)| Entry {
            name: n.clone(),
            score: s.0,
        };

        let out = if offset >= 0 {
            self.ranked
                .range(start_key..)
                .skip(offset as usize)
                .take(limit as usize)
                .map(|(k, _)| to_entry(k))
                .collect::<Vec<_>>()
        } else {
            // Window reaches back before the start key: take the closest
            // elements from below it (reversed range, O(|offset|)) and top up
            // from the forward range.
            let mut out: Vec<Entry> = self
                .ranked
                .range(..&start_key)
                .rev()
                .take((-offset) as usize)
                .take(limit as usize)
                .map(|(k, _)| to_entry(k))
                .collect();
            out.reverse();
            if out.len() < limit as usize {
                out.extend(
                    self.ranked
                        .range(&start_key..)
                        .take(limit as usize - out.len())
                        .map(|(k, _)| to_entry(k)),
                );
            }
            out
        };
        out
    }
}
