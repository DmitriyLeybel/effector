use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use crate::{
    browser_snapshot::{Baseline, StoredSnapshot},
    operation_tasks::OperationTasks,
    protocol::{
        Capabilities, CapabilitiesChangedMessage, DomainError, ImplementationEntry, ReadyMessage,
        validate_capability_implementations,
    },
    references::{
        BrowserObjectRef, BrowserSnapshotRef, CursorRef, GroupRef, SnapshotReferences, TabRef,
        WindowRef,
    },
    retention::{
        Clock, RemovedRecord, RetainedStore, RetentionCandidate, RetentionError, RetentionPolicy,
        SystemClock,
    },
};

const MAX_RETAINED_SNAPSHOTS: usize = 16;
const MAX_RETAINED_BYTES: usize = 32 * 1024 * 1024;
const SNAPSHOT_TTL: Duration = Duration::from_secs(2 * 60);

#[derive(Clone)]
pub(crate) struct BrokerRuntime {
    inner: Arc<Mutex<RuntimeState>>,
    clock: Arc<dyn Clock>,
    operations: OperationTasks,
}

pub(crate) struct RuntimeState {
    pub(crate) connected: Option<ConnectedBrowser>,
    pub(crate) references: ReferenceRegistry,
    pub(crate) retention: RuntimeRetention,
    pub(crate) latest_model_revision: Option<u64>,
    pub(crate) latest_model_fingerprint: Option<Vec<u8>>,
}

#[derive(Clone)]
pub(crate) struct ConnectedBrowser {
    pub(crate) browser_instance_id: String,
    pub(crate) _extension_id: String,
    pub(crate) _extension_version: String,
    pub(crate) _user_agent: String,
    pub(crate) capability_revision: u64,
    pub(crate) capabilities: Capabilities,
    pub(crate) implementations: Vec<ImplementationEntry>,
}

#[derive(Clone, Default)]
pub(crate) struct ReferenceRegistry {
    windows: HashMap<String, ReferenceRecord<WindowRef>>,
    groups: HashMap<String, ReferenceRecord<GroupRef>>,
    tabs: HashMap<String, ReferenceRecord<TabRef>>,
}

#[derive(Clone)]
struct ReferenceRecord<R> {
    chrome_id: i32,
    public_ref: R,
}

pub(crate) struct RuntimeRetention {
    records: RetainedStore<RetentionKey, RetainedValue, RetentionClass>,
    reference_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RetentionKey {
    BrowserSnapshot(BrowserSnapshotRef),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RetentionClass {
    BrowserSnapshot,
}

enum RetainedValue {
    BrowserSnapshot(StoredSnapshot),
}

impl BrokerRuntime {
    pub(crate) fn new() -> Self {
        Self::with_clock(Arc::new(SystemClock))
    }

    pub(crate) fn with_clock(clock: Arc<dyn Clock>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(RuntimeState {
                connected: None,
                references: ReferenceRegistry::default(),
                retention: RuntimeRetention::new(),
                latest_model_revision: None,
                latest_model_fingerprint: None,
            })),
            clock,
            operations: OperationTasks::new(),
        }
    }

    pub(crate) fn now(&self) -> Instant {
        self.clock.now()
    }

    pub(crate) fn connect(&self, ready: ReadyMessage, implementations: Vec<ImplementationEntry>) {
        let mut state = self.state();
        state.clear_retained();
        state.connected = Some(ConnectedBrowser {
            browser_instance_id: ready.browser_instance_id,
            _extension_id: ready.extension_id,
            _extension_version: ready.extension_version,
            _user_agent: ready.user_agent,
            capability_revision: ready.capability_revision,
            capabilities: ready.capabilities,
            implementations,
        });
    }

    pub(crate) fn apply_capabilities(
        &self,
        changed: CapabilitiesChangedMessage,
    ) -> Result<bool, String> {
        let mut state = self.state();
        let Some(connected) = state.connected.as_mut() else {
            return Err("capabilities_changed arrived before ready".to_owned());
        };
        if connected.browser_instance_id != changed.browser_instance_id {
            return Err("capabilities_changed browser identity did not match ready".to_owned());
        }
        if changed.capability_revision <= connected.capability_revision {
            return Ok(false);
        }
        validate_capability_implementations(&changed.capabilities, &connected.implementations)
            .map_err(|error| error.to_string())?;
        connected.capability_revision = changed.capability_revision;
        connected.capabilities = changed.capabilities;
        Ok(true)
    }

    pub(crate) fn clear(&self) {
        self.operations.shutdown_and_clear();
        let mut state = self.state();
        state.connected = None;
        state.clear_retained();
    }

    pub(crate) fn browser_instance_id(&self) -> Option<String> {
        self.state()
            .connected
            .as_ref()
            .map(|connected| connected.browser_instance_id.clone())
    }

    pub(crate) fn require_snapshot_capability(&self) -> Result<ConnectedBrowser, DomainError> {
        let state = self.state();
        let connected = state.connected.clone().ok_or_else(|| {
            DomainError::new(
                "CAPABILITY_UNAVAILABLE",
                "The Chrome extension handshake is not complete.",
            )
        })?;
        if !connected.capabilities.browser_snapshot.effective {
            return Err(DomainError::new(
                "CAPABILITY_UNAVAILABLE",
                "Browser snapshots are not supported by the connected extension.",
            ));
        }
        Ok(connected)
    }

    pub(crate) fn state(&self) -> MutexGuard<'_, RuntimeState> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl RuntimeState {
    fn clear_retained(&mut self) {
        self.references = ReferenceRegistry::default();
        self.retention.clear();
        self.latest_model_revision = None;
        self.latest_model_fingerprint = None;
    }
}

impl ReferenceRegistry {
    pub(crate) fn reconcile(
        &mut self,
        baseline: &Baseline,
    ) -> Result<SnapshotReferences, DomainError> {
        let window_keys: HashSet<&str> = baseline
            .windows
            .iter()
            .map(|window| window.key.as_str())
            .collect();
        let group_keys: HashSet<&str> = baseline
            .groups
            .iter()
            .map(|group| group.key.as_str())
            .collect();
        let tab_keys: HashSet<&str> = baseline.tabs.iter().map(|tab| tab.key.as_str()).collect();
        self.windows
            .retain(|key, _| window_keys.contains(key.as_str()));
        self.groups
            .retain(|key, _| group_keys.contains(key.as_str()));
        self.tabs.retain(|key, _| tab_keys.contains(key.as_str()));

        for window in &baseline.windows {
            reconcile_reference(&mut self.windows, &window.key, window.id, "window")?;
        }
        for group in &baseline.groups {
            reconcile_reference(&mut self.groups, &group.key, group.id, "group")?;
        }
        for tab in &baseline.tabs {
            reconcile_reference(&mut self.tabs, &tab.key, tab.id, "tab")?;
        }

        Ok(SnapshotReferences::new(
            public_references(&self.windows),
            public_references(&self.groups),
            public_references(&self.tabs),
        ))
    }
}

fn reconcile_reference<R: BrowserObjectRef>(
    records: &mut HashMap<String, ReferenceRecord<R>>,
    key: &str,
    chrome_id: i32,
    kind: &str,
) -> Result<(), DomainError> {
    if let Some(existing) = records.get(key) {
        if existing.chrome_id != chrome_id {
            return Err(DomainError::new(
                "INTERNAL_ERROR",
                format!("The extension reused a {kind} incarnation key."),
            ));
        }
        return Ok(());
    }
    records.insert(
        key.to_owned(),
        ReferenceRecord {
            chrome_id,
            public_ref: R::issue(),
        },
    );
    Ok(())
}

fn public_references<R: Clone>(
    records: &HashMap<String, ReferenceRecord<R>>,
) -> HashMap<String, R> {
    records
        .iter()
        .map(|(key, record)| (key.clone(), record.public_ref.clone()))
        .collect()
}

impl RuntimeRetention {
    fn new() -> Self {
        Self {
            records: RetainedStore::new(MAX_RETAINED_BYTES),
            reference_bytes: 0,
        }
    }

    pub(crate) fn retain_browser_snapshot(
        &mut self,
        now: Instant,
        next_reference_bytes: usize,
        snapshot_ref: BrowserSnapshotRef,
        snapshot_bytes: usize,
        snapshot: StoredSnapshot,
    ) -> Result<(), DomainError> {
        let removed = self
            .records
            .insert(
                now,
                next_reference_bytes,
                RetentionCandidate {
                    key: RetentionKey::BrowserSnapshot(snapshot_ref),
                    class: RetentionClass::BrowserSnapshot,
                    policy: RetentionPolicy {
                        ttl: SNAPSHOT_TTL,
                        max_records: MAX_RETAINED_SNAPSHOTS,
                        max_bytes: MAX_RETAINED_BYTES,
                    },
                    retained_bytes: snapshot_bytes,
                    value: RetainedValue::BrowserSnapshot(snapshot),
                },
            )
            .map_err(map_retention_error)?;
        cleanup_removed(removed);
        self.reference_bytes = next_reference_bytes;
        Ok(())
    }

    pub(crate) fn expire(&mut self, now: Instant) {
        let removed = self.records.expire(now);
        cleanup_removed(removed);
    }

    pub(crate) fn browser_snapshot(
        &self,
        reference: &BrowserSnapshotRef,
    ) -> Option<&StoredSnapshot> {
        let key = RetentionKey::BrowserSnapshot(reference.clone());
        match self.records.get(&key) {
            Some(RetainedValue::BrowserSnapshot(snapshot)) => Some(snapshot),
            None => None,
        }
    }

    pub(crate) fn browser_snapshot_by_handle(
        &mut self,
        now: Instant,
        handle: &str,
        expected_browser_instance_id: &str,
    ) -> Result<&StoredSnapshot, DomainError> {
        self.expire(now);
        let reference = BrowserSnapshotRef::parse("browserSnapshotRef", handle)?;
        let snapshot = self.browser_snapshot(&reference).ok_or_else(|| {
            DomainError::new(
                "HANDLE_EXPIRED",
                "The browser snapshot reference is invalid or no longer retained.",
            )
        })?;
        if snapshot.browser_instance_id() != expected_browser_instance_id {
            return Err(DomainError::new(
                "HANDLE_EXPIRED",
                "The browser snapshot reference belongs to another browser instance.",
            ));
        }
        Ok(snapshot)
    }

    pub(crate) fn browser_snapshot_for_cursor(
        &self,
        cursor: &CursorRef,
    ) -> Option<(&StoredSnapshot, usize)> {
        self.records.values().find_map(|value| {
            let RetainedValue::BrowserSnapshot(snapshot) = value;
            snapshot
                .cursor_offset(cursor)
                .map(|offset| (snapshot, offset))
        })
    }

    pub(crate) fn clear(&mut self) {
        self.records.clear();
        self.reference_bytes = 0;
    }
}

fn cleanup_removed(records: Vec<RemovedRecord<RetentionKey, RetainedValue, RetentionClass>>) {
    for record in records {
        match (record.key, record.class, record.value) {
            (
                RetentionKey::BrowserSnapshot(_),
                RetentionClass::BrowserSnapshot,
                RetainedValue::BrowserSnapshot(_),
            ) => {}
        }
        let _ = record.retained_bytes;
    }
}

fn map_retention_error(error: RetentionError) -> DomainError {
    match error {
        RetentionError::CapacityExceeded => DomainError::new(
            "RESULT_TOO_LARGE",
            "The complete browser snapshot is too large to retain; narrow the query.",
        ),
        RetentionError::DuplicateKey | RetentionError::ArithmeticOverflow => DomainError::new(
            "INTERNAL_ERROR",
            "The browser snapshot could not be retained safely.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::browser_snapshot::Baseline;

    use super::ReferenceRegistry;

    fn baseline(tab_id: i32) -> Baseline {
        serde_json::from_value(json!({
            "browserInstanceId": "browser",
            "modelRevision": 1,
            "capturedAt": "2026-08-03T12:00:00Z",
            "supportsFrozenTabs": false,
            "supportsSharedTabGroups": false,
            "windows": [{"key":"window-key","id":1,"focused":true}],
            "groups": [],
            "tabs": [{
                "key":"tab-key","id":tab_id,"windowKey":"window-key","index":0,
                "active":true,"highlighted":true,"pinned":false,"discarded":false
            }]
        }))
        .unwrap()
    }

    #[test]
    fn incarnation_keys_cannot_retarget_another_chrome_id() {
        let mut registry = ReferenceRegistry::default();
        registry.reconcile(&baseline(10)).unwrap();
        let error = registry.reconcile(&baseline(11)).unwrap_err();
        assert_eq!(error.code, "INTERNAL_ERROR");
    }
}
