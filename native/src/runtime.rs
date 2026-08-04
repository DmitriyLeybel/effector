use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex, MutexGuard},
};

use uuid::Uuid;

use crate::{
    browser_snapshot::{Baseline, SnapshotReferences, StoredSnapshot},
    protocol::{
        Capabilities, CapabilitiesChangedMessage, DomainError, ImplementationEntry, ReadyMessage,
        validate_capability_implementations,
    },
};

#[derive(Clone)]
pub(crate) struct BrokerRuntime {
    inner: Arc<Mutex<RuntimeState>>,
}

pub(crate) struct RuntimeState {
    pub(crate) connected: Option<ConnectedBrowser>,
    pub(crate) references: ReferenceRegistry,
    pub(crate) snapshots: VecDeque<StoredSnapshot>,
    pub(crate) snapshot_bytes: usize,
    pub(crate) reference_bytes: usize,
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
    windows: HashMap<String, ReferenceRecord>,
    groups: HashMap<String, ReferenceRecord>,
    tabs: HashMap<String, ReferenceRecord>,
}

#[derive(Clone)]
struct ReferenceRecord {
    chrome_id: i32,
    public_ref: String,
}

impl BrokerRuntime {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(RuntimeState {
                connected: None,
                references: ReferenceRegistry::default(),
                snapshots: VecDeque::new(),
                snapshot_bytes: 0,
                reference_bytes: 0,
                latest_model_revision: None,
                latest_model_fingerprint: None,
            })),
        }
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
        self.snapshots.clear();
        self.snapshot_bytes = 0;
        self.reference_bytes = 0;
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
            reconcile_reference(&mut self.windows, &window.key, window.id, "win", "window")?;
        }
        for group in &baseline.groups {
            reconcile_reference(&mut self.groups, &group.key, group.id, "grp", "group")?;
        }
        for tab in &baseline.tabs {
            reconcile_reference(&mut self.tabs, &tab.key, tab.id, "tab", "tab")?;
        }

        Ok(SnapshotReferences {
            windows: public_references(&self.windows),
            groups: public_references(&self.groups),
            tabs: public_references(&self.tabs),
        })
    }
}

fn reconcile_reference(
    records: &mut HashMap<String, ReferenceRecord>,
    key: &str,
    chrome_id: i32,
    prefix: &str,
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
            public_ref: random_ref(prefix),
        },
    );
    Ok(())
}

fn public_references(records: &HashMap<String, ReferenceRecord>) -> HashMap<String, String> {
    records
        .iter()
        .map(|(key, record)| (key.clone(), record.public_ref.clone()))
        .collect()
}

pub(crate) fn random_ref(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
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
