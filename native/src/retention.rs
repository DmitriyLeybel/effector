use std::{
    borrow::Borrow,
    collections::VecDeque,
    time::{Duration, Instant},
};

pub(crate) trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct FakeClock {
    now: std::sync::Arc<std::sync::Mutex<Instant>>,
}

#[cfg(test)]
impl FakeClock {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            now: std::sync::Arc::new(std::sync::Mutex::new(now)),
        }
    }

    pub(crate) fn advance(&self, duration: Duration) {
        let mut now = self
            .now
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *now = now.checked_add(duration).expect("fake clock overflow");
    }
}

#[cfg(test)]
impl Clock for FakeClock {
    fn now(&self) -> Instant {
        *self
            .now
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RetentionPolicy {
    pub(crate) ttl: Duration,
    pub(crate) max_records: usize,
    pub(crate) max_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetentionError {
    DuplicateKey,
    CapacityExceeded,
    ArithmeticOverflow,
}

struct RetainedRecord<K, V, C> {
    key: K,
    class: C,
    value: V,
    created_at: Instant,
    ttl: Duration,
    retained_bytes: usize,
}

#[must_use = "removed records may require dependency cleanup"]
pub(crate) struct RemovedRecord<K, V, C> {
    pub(crate) key: K,
    pub(crate) class: C,
    pub(crate) value: V,
    pub(crate) retained_bytes: usize,
}

pub(crate) struct RetentionCandidate<K, V, C> {
    pub(crate) key: K,
    pub(crate) class: C,
    pub(crate) policy: RetentionPolicy,
    pub(crate) retained_bytes: usize,
    pub(crate) value: V,
}

pub(crate) struct RetainedStore<K, V, C> {
    aggregate_max_bytes: usize,
    records: VecDeque<RetainedRecord<K, V, C>>,
    retained_bytes: usize,
}

impl<K: Eq, V, C: Eq> RetainedStore<K, V, C> {
    pub(crate) fn new(aggregate_max_bytes: usize) -> Self {
        Self {
            aggregate_max_bytes,
            records: VecDeque::new(),
            retained_bytes: 0,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.records.clear();
        self.retained_bytes = 0;
    }

    #[must_use = "expired records may require dependency cleanup"]
    pub(crate) fn expire(&mut self, now: Instant) -> Vec<RemovedRecord<K, V, C>> {
        let mut removed = Vec::new();
        let mut index = 0;
        while index < self.records.len() {
            let record = &self.records[index];
            if now.saturating_duration_since(record.created_at) >= record.ttl {
                removed.push(self.remove_at(index));
            } else {
                index += 1;
            }
        }
        removed
    }

    pub(crate) fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Eq + ?Sized,
    {
        self.records
            .iter()
            .find(|record| record.key.borrow() == key)
            .map(|record| &record.value)
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &V> {
        self.records.iter().map(|record| &record.value)
    }

    #[must_use = "evicted records may require dependency cleanup"]
    pub(crate) fn insert(
        &mut self,
        now: Instant,
        fixed_bytes: usize,
        candidate: RetentionCandidate<K, V, C>,
    ) -> Result<Vec<RemovedRecord<K, V, C>>, RetentionError> {
        let RetentionCandidate {
            key,
            class,
            policy,
            retained_bytes,
            value,
        } = candidate;
        if self.records.iter().any(|record| record.key == key) {
            return Err(RetentionError::DuplicateKey);
        }
        if policy.ttl.is_zero()
            || policy.max_records == 0
            || policy.max_bytes == 0
            || self.aggregate_max_bytes == 0
        {
            return Err(RetentionError::CapacityExceeded);
        }
        if retained_bytes > policy.max_bytes {
            return Err(RetentionError::CapacityExceeded);
        }

        let candidate_aggregate_bytes = fixed_bytes
            .checked_add(retained_bytes)
            .ok_or(RetentionError::ArithmeticOverflow)?;
        if candidate_aggregate_bytes > self.aggregate_max_bytes {
            return Err(RetentionError::CapacityExceeded);
        }

        let mut class_count = 0usize;
        let mut class_bytes = 0usize;
        for record in self.records.iter().filter(|record| record.class == class) {
            class_count = class_count
                .checked_add(1)
                .ok_or(RetentionError::ArithmeticOverflow)?;
            class_bytes = class_bytes
                .checked_add(record.retained_bytes)
                .ok_or(RetentionError::ArithmeticOverflow)?;
        }
        class_count
            .checked_add(1)
            .ok_or(RetentionError::ArithmeticOverflow)?;
        class_bytes
            .checked_add(retained_bytes)
            .ok_or(RetentionError::ArithmeticOverflow)?;

        let store_bytes_with_candidate = self
            .retained_bytes
            .checked_add(retained_bytes)
            .ok_or(RetentionError::ArithmeticOverflow)?;
        fixed_bytes
            .checked_add(store_bytes_with_candidate)
            .ok_or(RetentionError::ArithmeticOverflow)?;

        let mut removed = self.expire(now);
        let (mut class_count, mut class_bytes) = self.class_usage(&class);
        while class_count >= policy.max_records || class_bytes > policy.max_bytes - retained_bytes {
            let index = self.records.iter().position(|record| record.class == class);
            let Some(index) = index else {
                unreachable!("class usage requires a matching retained record");
            };
            let record = self.remove_at(index);
            class_count -= 1;
            class_bytes -= record.retained_bytes;
            removed.push(record);
        }

        let aggregate_store_capacity = self.aggregate_max_bytes - fixed_bytes;
        while self.retained_bytes > aggregate_store_capacity - retained_bytes {
            removed.push(self.remove_at(0));
        }

        self.retained_bytes += retained_bytes;
        self.records.push_back(RetainedRecord {
            key,
            class,
            value,
            created_at: now,
            ttl: policy.ttl,
            retained_bytes,
        });
        Ok(removed)
    }

    fn class_usage(&self, class: &C) -> (usize, usize) {
        self.records
            .iter()
            .filter(|record| &record.class == class)
            .fold((0, 0), |(count, bytes), record| {
                (count + 1, bytes + record.retained_bytes)
            })
    }

    fn remove_at(&mut self, index: usize) -> RemovedRecord<K, V, C> {
        let record = match self.records.remove(index) {
            Some(record) => record,
            None => unreachable!("retained record index was validated before removal"),
        };
        self.retained_bytes -= record.retained_bytes;
        RemovedRecord {
            key: record.key,
            class: record.class,
            value: record.value,
            retained_bytes: record.retained_bytes,
        }
    }

    #[cfg(test)]
    pub(crate) fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.records.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Clock, FakeClock, RetainedStore, RetentionCandidate, RetentionError, RetentionPolicy,
        SystemClock,
    };
    use std::time::{Duration, Instant};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Class {
        Snapshot,
        Plan,
    }

    fn policy(ttl_secs: u64, max_records: usize, max_bytes: usize) -> RetentionPolicy {
        RetentionPolicy {
            ttl: Duration::from_secs(ttl_secs),
            max_records,
            max_bytes,
        }
    }

    fn candidate<K, V>(
        key: K,
        class: Class,
        policy: RetentionPolicy,
        retained_bytes: usize,
        value: V,
    ) -> RetentionCandidate<K, V, Class> {
        RetentionCandidate {
            key,
            class,
            policy,
            retained_bytes,
            value,
        }
    }

    #[test]
    fn exact_ttl_expires_due_records_beyond_a_live_prefix() {
        let start = Instant::now();
        let mut store = RetainedStore::new(100);
        store
            .insert(
                start,
                0,
                candidate("slow", Class::Snapshot, policy(20, 2, 100), 4, 1),
            )
            .unwrap();
        store
            .insert(
                start + Duration::from_secs(1),
                0,
                candidate("fast", Class::Plan, policy(5, 2, 100), 6, 2),
            )
            .unwrap();

        let removed = store.expire(start + Duration::from_secs(6) - Duration::from_nanos(1));
        assert!(removed.is_empty());

        let removed = store.expire(start + Duration::from_secs(6));
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].key, "fast");
        assert_eq!(store.get("slow"), Some(&1));
        assert_eq!(store.get("fast"), None);
        assert_eq!(store.retained_bytes(), 4);
    }

    #[test]
    fn reads_do_not_refresh_ttl_or_fifo_order() {
        let start = Instant::now();
        let mut store = RetainedStore::new(100);
        let policy = policy(10, 2, 100);
        store
            .insert(start, 0, candidate("a", Class::Snapshot, policy, 1, 1))
            .unwrap();
        store
            .insert(
                start + Duration::from_secs(1),
                0,
                candidate("b", Class::Snapshot, policy, 1, 2),
            )
            .unwrap();

        assert_eq!(store.get("a"), Some(&1));
        assert_eq!(store.values().copied().collect::<Vec<_>>(), vec![1, 2]);
        let removed = store
            .insert(
                start + Duration::from_secs(2),
                0,
                candidate("c", Class::Snapshot, policy, 1, 3),
            )
            .unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].key, "a");

        assert_eq!(store.get("b"), Some(&2));
        let removed = store.expire(start + Duration::from_secs(11));
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].key, "b");
    }

    #[test]
    fn per_class_count_and_byte_limits_evict_only_that_class() {
        let now = Instant::now();
        let mut store = RetainedStore::new(100);
        let count_policy = policy(10, 2, 100);
        store
            .insert(now, 0, candidate("s1", Class::Snapshot, count_policy, 2, 1))
            .unwrap();
        store
            .insert(now, 0, candidate("p1", Class::Plan, count_policy, 10, 10))
            .unwrap();
        store
            .insert(now, 0, candidate("s2", Class::Snapshot, count_policy, 2, 2))
            .unwrap();

        let removed = store
            .insert(now, 0, candidate("s3", Class::Snapshot, count_policy, 2, 3))
            .unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].key, "s1");

        let byte_policy = policy(10, 10, 4);
        let removed = store
            .insert(now, 0, candidate("s4", Class::Snapshot, byte_policy, 3, 4))
            .unwrap();
        assert_eq!(
            removed.iter().map(|record| record.key).collect::<Vec<_>>(),
            vec!["s2", "s3"]
        );
        assert_eq!(store.get("p1"), Some(&10));
        assert_eq!(store.get("s4"), Some(&4));
    }

    #[test]
    fn aggregate_limit_evicts_globally_oldest_across_classes() {
        let now = Instant::now();
        let mut store = RetainedStore::new(6);
        let policy = policy(10, 10, 100);
        store
            .insert(now, 0, candidate("s1", Class::Snapshot, policy, 2, 1))
            .unwrap();
        store
            .insert(now, 0, candidate("p1", Class::Plan, policy, 2, 2))
            .unwrap();
        store
            .insert(now, 0, candidate("s2", Class::Snapshot, policy, 2, 3))
            .unwrap();

        let removed = store
            .insert(now, 0, candidate("p2", Class::Plan, policy, 2, 4))
            .unwrap();

        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].key, "s1");
        assert_eq!(removed[0].class, Class::Snapshot);
        assert_eq!(store.values().copied().collect::<Vec<_>>(), vec![2, 3, 4]);
        assert_eq!(store.retained_bytes(), 6);
    }

    #[test]
    fn fixed_bytes_create_global_replacement_pressure() {
        let now = Instant::now();
        let mut store = RetainedStore::new(10);
        let policy = policy(10, 10, 100);
        store
            .insert(now, 0, candidate("s1", Class::Snapshot, policy, 2, 1))
            .unwrap();
        store
            .insert(now, 0, candidate("p1", Class::Plan, policy, 2, 2))
            .unwrap();

        let removed = store
            .insert(now, 6, candidate("s2", Class::Snapshot, policy, 2, 3))
            .unwrap();

        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].key, "s1");
        assert_eq!(store.retained_bytes(), 4);
        assert_eq!(store.values().copied().collect::<Vec<_>>(), vec![2, 3]);
    }

    #[test]
    fn exact_boundary_is_accepted_and_rejections_are_atomic() {
        let start = Instant::now();
        let mut store = RetainedStore::new(10);
        store
            .insert(
                start,
                4,
                candidate("exact", Class::Snapshot, policy(1, 2, 6), 6, 1),
            )
            .unwrap();
        assert_eq!(store.retained_bytes(), 6);

        assert!(matches!(
            store.insert(
                start + Duration::from_secs(2),
                0,
                candidate("exact", Class::Snapshot, policy(10, 2, 6), 1, 2),
            ),
            Err(RetentionError::DuplicateKey)
        ));
        assert!(matches!(
            store.insert(
                start + Duration::from_secs(2),
                0,
                candidate("class-over", Class::Plan, policy(10, 2, 6), 7, 2),
            ),
            Err(RetentionError::CapacityExceeded)
        ));
        assert!(matches!(
            store.insert(
                start + Duration::from_secs(2),
                4,
                candidate("one-over", Class::Plan, policy(10, 2, 7), 7, 2),
            ),
            Err(RetentionError::CapacityExceeded)
        ));
        assert_eq!(store.get("exact"), Some(&1));
        assert_eq!(store.len(), 1);
        assert_eq!(store.retained_bytes(), 6);

        let mut overflow_store = RetainedStore::new(usize::MAX);
        overflow_store
            .insert(
                start,
                0,
                candidate("present", Class::Snapshot, policy(1, 2, usize::MAX), 1, 1),
            )
            .unwrap();
        assert!(matches!(
            overflow_store.insert(
                start + Duration::from_secs(2),
                0,
                candidate(
                    "overflow",
                    Class::Snapshot,
                    policy(10, 2, usize::MAX),
                    usize::MAX,
                    2,
                ),
            ),
            Err(RetentionError::ArithmeticOverflow)
        ));
        assert_eq!(overflow_store.get("present"), Some(&1));

        let mut zero_limit = RetainedStore::new(10);
        assert!(matches!(
            zero_limit.insert(
                start,
                0,
                candidate("zero", Class::Snapshot, policy(10, 0, 10), 1, ()),
            ),
            Err(RetentionError::CapacityExceeded)
        ));
    }

    #[test]
    fn removals_return_owned_cleanup_data() {
        let start = Instant::now();
        let mut store = RetainedStore::new(4);
        let policy = policy(2, 10, 100);
        store
            .insert(
                start,
                0,
                candidate(
                    String::from("snapshot"),
                    Class::Snapshot,
                    policy,
                    4,
                    String::from("payload"),
                ),
            )
            .unwrap();

        let mut removed = store
            .insert(
                start + Duration::from_secs(1),
                0,
                candidate(
                    String::from("plan"),
                    Class::Plan,
                    policy,
                    4,
                    String::from("replacement"),
                ),
            )
            .unwrap();
        let record = removed.pop().unwrap();
        assert_eq!(record.key, "snapshot");
        assert_eq!(record.class, Class::Snapshot);
        assert_eq!(record.value, "payload");
        assert_eq!(record.retained_bytes, 4);

        let mut removed = store.expire(start + Duration::from_secs(3));
        let record = removed.pop().unwrap();
        assert_eq!(record.key, "plan");
        assert_eq!(record.class, Class::Plan);
        assert_eq!(record.value, "replacement");
        assert_eq!(record.retained_bytes, 4);
        assert_eq!(store.retained_bytes(), 0);
    }

    #[test]
    fn fake_clock_clones_advance_deterministically() {
        let start = Instant::now();
        let _ = SystemClock.now();
        let clock = FakeClock::new(start);
        let clone = clock.clone();

        clone.advance(Duration::from_millis(25));

        assert_eq!(clock.now(), start + Duration::from_millis(25));
        assert_eq!(clone.now(), clock.now());
    }

    #[test]
    fn clear_resets_records_and_exact_accounting() {
        let now = Instant::now();
        let mut store = RetainedStore::new(20);
        store
            .insert(
                now,
                0,
                candidate("snapshot", Class::Snapshot, policy(10, 2, 20), 7, ()),
            )
            .unwrap();

        store.clear();

        assert_eq!(store.len(), 0);
        assert_eq!(store.retained_bytes(), 0);
        assert!(store.values().next().is_none());
    }
}
