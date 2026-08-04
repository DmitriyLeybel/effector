use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    broker::{BrowserRequestError, BrowserRequestHandle},
    protocol::{BrowserMethod, DomainError},
    runtime::{BrokerRuntime, random_ref},
};

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 250;
const MAX_TARGET_TABS: usize = 250;
const MAX_QUERY_BYTES: usize = 4096;
const MAX_CURSOR_RECORDS: usize = 10_000;
const MAX_RETAINED_SNAPSHOTS: usize = 16;
const MAX_RETAINED_BYTES: usize = 32 * 1024 * 1024;
const MAX_RESULT_BYTES: usize = 4 * 1024 * 1024;
const SNAPSHOT_TTL: Duration = Duration::from_secs(2 * 60);

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(transparent)]
pub(crate) struct WindowRef(String);

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(transparent)]
pub(crate) struct GroupRef(String);

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(transparent)]
pub(crate) struct TabRef(String);

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SnapshotDetail {
    Counts,
    Compact,
    Full,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BrowserSnapshotParams {
    /// Restrict results to one exact window.
    #[schemars(with = "WindowRef")]
    pub window_ref: Option<WindowRef>,
    /// Restrict results to one exact Tab Group.
    #[schemars(with = "GroupRef")]
    pub group_ref: Option<GroupRef>,
    /// Restrict results to these exact tabs.
    #[schemars(with = "Vec<TabRef>", length(min = 1, max = 250))]
    pub tab_refs: Option<Vec<TabRef>>,
    /// Match tabs whose active state equals this value.
    #[schemars(with = "bool")]
    pub active: Option<bool>,
    /// Match tabs whose pinned state equals this value.
    #[schemars(with = "bool")]
    pub pinned: Option<bool>,
    /// Match tabs whose discarded state equals this value.
    #[schemars(with = "bool")]
    pub discarded: Option<bool>,
    /// Match tabs whose frozen state equals this value.
    #[schemars(with = "bool")]
    pub frozen: Option<bool>,
    /// Case-insensitive non-empty substring matched against tab title or URL.
    #[schemars(with = "String", length(min = 1, max = 4096))]
    pub query: Option<String>,
    /// Return counts, compact identification data, or full metadata. Defaults to compact.
    #[schemars(with = "SnapshotDetail")]
    pub detail: Option<SnapshotDetail>,
    /// Maximum matching tabs in this page. Defaults to 100.
    #[schemars(with = "usize", range(min = 1, max = 250))]
    pub limit: Option<usize>,
    /// Continue an immutable retained browser snapshot.
    #[schemars(with = "String")]
    pub cursor: Option<String>,
}

impl BrowserSnapshotParams {
    fn validate(&self) -> Result<(), DomainError> {
        if self.cursor.is_some() {
            let only_cursor = self.window_ref.is_none()
                && self.group_ref.is_none()
                && self.tab_refs.is_none()
                && self.active.is_none()
                && self.pinned.is_none()
                && self.discarded.is_none()
                && self.frozen.is_none()
                && self.query.is_none()
                && self.detail.is_none()
                && self.limit.is_none();
            if !only_cursor {
                return Err(DomainError::invalid_argument(
                    "A cursor call may contain only cursor.",
                ));
            }
            validate_ref("cursor", self.cursor.as_deref(), "cur")?;
            return Ok(());
        }

        let target_count = usize::from(self.window_ref.is_some())
            + usize::from(self.group_ref.is_some())
            + usize::from(self.tab_refs.is_some());
        if target_count > 1 {
            return Err(DomainError::invalid_argument(
                "windowRef, groupRef, and tabRefs are mutually exclusive.",
            ));
        }
        validate_ref(
            "windowRef",
            self.window_ref.as_ref().map(|value| value.0.as_str()),
            "win",
        )?;
        validate_ref(
            "groupRef",
            self.group_ref.as_ref().map(|value| value.0.as_str()),
            "grp",
        )?;
        if let Some(tab_refs) = &self.tab_refs {
            if tab_refs.is_empty() {
                return Err(DomainError::invalid_argument(
                    "tabRefs must contain at least one reference.",
                ));
            }
            if tab_refs.len() > MAX_TARGET_TABS {
                return Err(DomainError::invalid_argument(
                    "tabRefs may contain at most 250 references.",
                ));
            }
            let mut unique = HashSet::new();
            for tab_ref in tab_refs {
                validate_ref("tabRefs", Some(&tab_ref.0), "tab")?;
                if !unique.insert(tab_ref.0.as_str()) {
                    return Err(DomainError::invalid_argument(
                        "tabRefs must not contain duplicates.",
                    ));
                }
            }
        }
        if self
            .query
            .as_ref()
            .is_some_and(|query| query.trim().is_empty())
        {
            return Err(DomainError::invalid_argument("query must be non-empty."));
        }
        if self
            .query
            .as_ref()
            .is_some_and(|query| query.len() > MAX_QUERY_BYTES)
        {
            return Err(DomainError::invalid_argument(
                "query may contain at most 4096 UTF-8 bytes.",
            ));
        }
        if self
            .limit
            .is_some_and(|limit| !(1..=MAX_LIMIT).contains(&limit))
        {
            return Err(DomainError::invalid_argument(
                "limit must be between 1 and 250.",
            ));
        }
        if self.detail == Some(SnapshotDetail::Counts) && self.limit.is_some() {
            return Err(DomainError::invalid_argument(
                "detail counts does not accept limit or cursor.",
            ));
        }
        Ok(())
    }
}

pub(crate) fn decode_params(
    value: serde_json::Value,
) -> Result<BrowserSnapshotParams, DomainError> {
    let object = value.as_object().ok_or_else(|| {
        DomainError::invalid_argument("browser.snapshot arguments must be an object.")
    })?;
    if let Some(field) = object
        .iter()
        .find_map(|(field, value)| value.is_null().then_some(field))
    {
        return Err(DomainError::invalid_argument(format!(
            "{field} must be omitted instead of null."
        )));
    }
    serde_json::from_value(value).map_err(|_| {
        DomainError::invalid_argument("browser.snapshot arguments did not match the tool schema.")
    })
}

fn validate_ref(field: &str, value: Option<&str>, prefix: &str) -> Result<(), DomainError> {
    if let Some(value) = value
        && (value.len() <= prefix.len() + 1 || !value.starts_with(&format!("{prefix}_")))
    {
        return Err(DomainError::invalid_argument(format!(
            "{field} is not a valid {prefix} reference."
        )));
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Baseline {
    pub browser_instance_id: String,
    pub model_revision: u64,
    pub captured_at: String,
    pub supports_frozen_tabs: bool,
    pub supports_shared_tab_groups: bool,
    pub windows: Vec<BaselineWindow>,
    pub groups: Vec<BaselineGroup>,
    pub tabs: Vec<BaselineTab>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BaselineWindow {
    pub key: String,
    pub id: i32,
    pub focused: bool,
    pub top: Option<i32>,
    pub left: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    #[serde(rename = "type")]
    pub window_type: Option<String>,
    pub state: Option<String>,
    pub always_on_top: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BaselineGroup {
    pub key: String,
    pub id: i32,
    pub window_key: String,
    pub title: Option<String>,
    pub color: String,
    pub collapsed: bool,
    pub shared: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BaselineTab {
    pub key: String,
    pub id: i32,
    pub window_key: String,
    pub group_key: Option<String>,
    pub index: i32,
    pub title: Option<String>,
    pub url: Option<String>,
    pub pending_url: Option<String>,
    pub active: bool,
    pub highlighted: bool,
    pub pinned: bool,
    pub audible: Option<bool>,
    pub muted: Option<bool>,
    pub status: Option<String>,
    pub discarded: bool,
    pub frozen: Option<bool>,
    pub auto_discardable: Option<bool>,
    pub last_accessed: Option<f64>,
    pub opener_key: Option<String>,
    pub fav_icon_url: Option<String>,
}

impl Baseline {
    fn validate(&self, expected_browser_instance_id: &str) -> Result<(), DomainError> {
        if self.browser_instance_id != expected_browser_instance_id {
            return Err(invalid_baseline(
                "The snapshot browser identity did not match the connected browser.",
            ));
        }
        if self.captured_at.trim().is_empty() {
            return Err(invalid_baseline("The snapshot omitted capturedAt."));
        }
        if self.model_revision == 0 {
            return Err(invalid_baseline(
                "The snapshot modelRevision must be at least 1.",
            ));
        }

        let mut all_keys = HashSet::new();
        let mut window_ids = HashSet::new();
        let mut window_keys = HashSet::new();
        for window in &self.windows {
            if window.key.is_empty()
                || !all_keys.insert(window.key.as_str())
                || !window_keys.insert(window.key.as_str())
                || !window_ids.insert(window.id)
            {
                return Err(invalid_baseline(
                    "The snapshot contained duplicate window identity.",
                ));
            }
        }

        let mut group_ids = HashSet::new();
        let mut group_windows = HashMap::new();
        for group in &self.groups {
            if group.key.is_empty()
                || !all_keys.insert(group.key.as_str())
                || !group_ids.insert(group.id)
            {
                return Err(invalid_baseline(
                    "The snapshot contained duplicate group identity.",
                ));
            }
            if !window_keys.contains(group.window_key.as_str()) {
                return Err(invalid_baseline("A group referenced an unknown window."));
            }
            group_windows.insert(group.key.as_str(), group.window_key.as_str());
        }

        let mut tab_ids = HashSet::new();
        let mut tab_keys = HashSet::new();
        let mut indexes = HashSet::new();
        for tab in &self.tabs {
            if tab.key.is_empty()
                || !all_keys.insert(tab.key.as_str())
                || !tab_keys.insert(tab.key.as_str())
                || !tab_ids.insert(tab.id)
            {
                return Err(invalid_baseline(
                    "The snapshot contained duplicate tab identity.",
                ));
            }
            if tab.index < 0 || !indexes.insert((tab.window_key.as_str(), tab.index)) {
                return Err(invalid_baseline(
                    "The snapshot contained invalid tab indexes.",
                ));
            }
            if !window_keys.contains(tab.window_key.as_str()) {
                return Err(invalid_baseline("A tab referenced an unknown window."));
            }
            if let Some(group_key) = tab.group_key.as_deref()
                && group_windows.get(group_key).copied() != Some(tab.window_key.as_str())
            {
                return Err(invalid_baseline(
                    "A tab referenced an unknown group or a group in another window.",
                ));
            }
        }
        for window_key in &window_keys {
            let mut tabs: Vec<&BaselineTab> = self
                .tabs
                .iter()
                .filter(|tab| tab.window_key == *window_key)
                .collect();
            tabs.sort_by_key(|tab| tab.index);
            let mut closed_groups = HashSet::new();
            let mut current_group: Option<&str> = None;
            for (expected_index, tab) in tabs.into_iter().enumerate() {
                if tab.index != expected_index as i32 {
                    return Err(invalid_baseline(
                        "Tab indexes were not contiguous within a window.",
                    ));
                }
                let next_group = tab.group_key.as_deref();
                if next_group != current_group {
                    if let Some(group) = current_group {
                        closed_groups.insert(group);
                    }
                    if next_group.is_some_and(|group| closed_groups.contains(group)) {
                        return Err(invalid_baseline(
                            "A tab group did not occupy one contiguous tab-strip range.",
                        ));
                    }
                    current_group = next_group;
                }
            }
        }
        for tab in &self.tabs {
            if tab
                .opener_key
                .as_deref()
                .is_some_and(|opener| !tab_keys.contains(opener))
            {
                return Err(invalid_baseline("A tab referenced an unknown opener."));
            }
        }
        if self.supports_frozen_tabs {
            if self.tabs.iter().any(|tab| tab.frozen.is_none()) {
                return Err(invalid_baseline(
                    "Frozen-tab support was declared without complete tab values.",
                ));
            }
        } else if self.tabs.iter().any(|tab| tab.frozen.is_some()) {
            return Err(invalid_baseline(
                "Frozen-tab values were returned without declared support.",
            ));
        }
        if self.supports_shared_tab_groups {
            if self.groups.iter().any(|group| group.shared.is_none()) {
                return Err(invalid_baseline(
                    "Shared-group support was declared without complete group values.",
                ));
            }
        } else if self.groups.iter().any(|group| group.shared.is_some()) {
            return Err(invalid_baseline(
                "Shared-group values were returned without declared support.",
            ));
        }
        Ok(())
    }

    fn fingerprint(&self) -> Result<Vec<u8>, DomainError> {
        let mut model = self.clone();
        model.captured_at.clear();
        serde_json::to_vec(&model)
            .map_err(|_| invalid_baseline("The browser model could not be fingerprinted."))
    }
}

fn invalid_baseline(message: &str) -> DomainError {
    DomainError::new("INTERNAL_ERROR", message)
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SnapshotReferences {
    pub(crate) windows: HashMap<String, String>,
    pub(crate) groups: HashMap<String, String>,
    pub(crate) tabs: HashMap<String, String>,
}

pub(crate) struct StoredSnapshot {
    snapshot_ref: String,
    created_at: Instant,
    retained_bytes: usize,
    baseline: Baseline,
    references: SnapshotReferences,
    matched_tab_keys: Vec<String>,
    detail: SnapshotDetail,
    limit: usize,
    cursors: HashMap<String, usize>,
    cursor_by_offset: HashMap<usize, String>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub(crate) enum BrowserSnapshotToolOutput {
    Counts(SnapshotCounts),
    Snapshot(SnapshotPage),
    Error(DomainError),
}

impl BrowserSnapshotToolOutput {
    pub(crate) fn summary(&self) -> String {
        match self {
            Self::Counts(counts) => format!(
                "Chrome is reachable with {} windows, {} groups, and {} tabs.",
                counts.window_count, counts.group_count, counts.tab_count
            ),
            Self::Snapshot(snapshot) => format!(
                "Returned {} of {} matching tabs.",
                snapshot
                    .windows
                    .iter()
                    .flat_map(|window| &window.items)
                    .map(|item| match item {
                        SnapshotItem::Group(group) => group.tabs.len(),
                        SnapshotItem::Tab(_) => 1,
                    })
                    .sum::<usize>(),
                snapshot.total_matched
            ),
            Self::Error(error) => error.message.clone(),
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SnapshotCounts {
    window_count: usize,
    group_count: usize,
    tab_count: usize,
    discarded_tab_count: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SnapshotPage {
    browser_snapshot_ref: String,
    captured_at: String,
    total_matched: usize,
    windows: Vec<SnapshotWindow>,
    next_cursor: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SnapshotWindow {
    #[serde(rename = "ref")]
    reference: String,
    #[serde(skip_serializing_if = "is_false")]
    focused: bool,
    items: Vec<SnapshotItem>,
    #[serde(skip_serializing_if = "is_false")]
    partial: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    top: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    left: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    width: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    height: Option<i32>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    window_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    always_on_top: Option<bool>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
enum SnapshotItem {
    Group(SnapshotGroupItem),
    Tab(SnapshotTab),
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SnapshotGroupItem {
    group: SnapshotGroup,
    tabs: Vec<SnapshotTab>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SnapshotGroup {
    #[serde(rename = "ref")]
    reference: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    color: String,
    #[serde(skip_serializing_if = "is_false")]
    partial: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    collapsed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    shared: Option<bool>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SnapshotTab {
    #[serde(rename = "ref")]
    reference: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "is_false")]
    active: bool,
    #[serde(skip_serializing_if = "is_false")]
    pinned: bool,
    #[serde(skip_serializing_if = "is_false")]
    discarded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    frozen: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pending_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    highlighted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    audible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    muted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auto_discardable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_accessed: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    opener_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fav_icon_url: Option<String>,
}

fn is_false(value: &bool) -> bool {
    !value
}

pub(crate) async fn execute(
    runtime: &BrokerRuntime,
    browser: &BrowserRequestHandle,
    params: BrowserSnapshotParams,
) -> Result<BrowserSnapshotToolOutput, DomainError> {
    params.validate()?;
    let connected = runtime.require_snapshot_capability()?;
    if let Some(cursor) = params.cursor {
        return continue_snapshot(runtime, &cursor).map(BrowserSnapshotToolOutput::Snapshot);
    }

    let raw = browser
        .request(BrowserMethod::BrowserSnapshot, serde_json::json!({}))
        .await
        .map_err(map_browser_error)?;
    if contains_null(&raw) {
        return Err(invalid_baseline(
            "The extension returned null in a browser snapshot baseline.",
        ));
    }
    let raw_size = serde_json::to_vec(&raw)
        .map_err(|_| invalid_baseline("The browser snapshot baseline could not be measured."))?
        .len();
    if raw_size > MAX_RETAINED_BYTES {
        return Err(DomainError::new(
            "RESULT_TOO_LARGE",
            "The complete browser snapshot is too large to retain; narrow the browser state.",
        ));
    }
    let baseline: Baseline = serde_json::from_value(raw).map_err(|_| {
        invalid_baseline("The extension returned an invalid browser snapshot baseline.")
    })?;
    baseline.validate(&connected.browser_instance_id)?;
    let model_fingerprint = baseline.fingerprint()?;
    let mut state = runtime.state();
    let Some(current) = state.connected.as_ref() else {
        return Err(DomainError::new(
            "CAPABILITY_UNAVAILABLE",
            "The Chrome extension disconnected during snapshot capture.",
        ));
    };
    if current.browser_instance_id != baseline.browser_instance_id {
        return Err(invalid_baseline(
            "The connected browser changed during snapshot capture.",
        ));
    }
    if !current.capabilities.browser_snapshot.effective {
        return Err(DomainError::new(
            "CAPABILITY_UNAVAILABLE",
            "Browser snapshot capability changed during capture.",
        ));
    }
    if state
        .latest_model_revision
        .is_some_and(|revision| baseline.model_revision < revision)
    {
        return Err(invalid_baseline(
            "The extension returned an older browser model revision.",
        ));
    }
    if state.latest_model_revision == Some(baseline.model_revision)
        && state.latest_model_fingerprint.as_ref() != Some(&model_fingerprint)
    {
        return Err(invalid_baseline(
            "The extension changed browser state without advancing modelRevision.",
        ));
    }
    if current.capabilities.frozen_tabs != baseline.supports_frozen_tabs
        || current.capabilities.shared_tab_groups != baseline.supports_shared_tab_groups
    {
        return Err(invalid_baseline(
            "The snapshot support flags did not match the negotiated capabilities.",
        ));
    }
    if params.frozen.is_some()
        && (!current.capabilities.frozen_tabs || !baseline.supports_frozen_tabs)
    {
        return Err(DomainError::new(
            "CAPABILITY_UNAVAILABLE",
            "Frozen-tab filtering is not supported by the connected Chrome version.",
        ));
    }
    let mut next_references = state.references.clone();
    let references = next_references.reconcile(&baseline)?;
    let matched = matched_tabs(&baseline, &references, &params)?;
    let reference_bytes = serde_json::to_vec(&references)
        .map_err(|_| invalid_baseline("The browser references could not be measured."))?
        .len();
    if reference_bytes > MAX_RETAINED_BYTES {
        return Err(DomainError::new(
            "RESULT_TOO_LARGE",
            "The browser reference set is too large to retain; narrow the browser state.",
        ));
    }
    if params.detail == Some(SnapshotDetail::Counts) {
        state.latest_model_revision = Some(baseline.model_revision);
        state.latest_model_fingerprint = Some(model_fingerprint);
        return Ok(BrowserSnapshotToolOutput::Counts(counts(
            &baseline, &matched,
        )));
    }

    let mut stored = StoredSnapshot {
        snapshot_ref: random_ref("bs"),
        created_at: Instant::now(),
        retained_bytes: 0,
        baseline,
        references,
        matched_tab_keys: matched,
        detail: params.detail.unwrap_or(SnapshotDetail::Compact),
        limit: params.limit.unwrap_or(DEFAULT_LIMIT),
        cursors: HashMap::new(),
        cursor_by_offset: HashMap::new(),
    };
    let cursor_count = stored
        .matched_tab_keys
        .len()
        .saturating_sub(1)
        .checked_div(stored.limit)
        .unwrap_or(0);
    if cursor_count > MAX_CURSOR_RECORDS {
        return Err(DomainError::new(
            "RESULT_TOO_LARGE",
            "The browser snapshot needs too many cursor records; narrow the request.",
        ));
    }
    prepare_cursors(&mut stored);
    let retained_bytes = retained_size(&stored)?;
    if reference_bytes + retained_bytes > MAX_RETAINED_BYTES {
        return Err(DomainError::new(
            "RESULT_TOO_LARGE",
            "The complete browser snapshot is too large to retain; narrow the query.",
        ));
    }
    stored.retained_bytes = retained_bytes;
    let page = render_page(&mut stored, 0)?;
    remove_expired(&mut state);
    while state.snapshots.len() >= MAX_RETAINED_SNAPSHOTS
        || reference_bytes + state.snapshot_bytes + retained_bytes > MAX_RETAINED_BYTES
    {
        evict_oldest(&mut state);
    }
    state.references = next_references;
    state.reference_bytes = reference_bytes;
    state.latest_model_revision = Some(stored.baseline.model_revision);
    state.latest_model_fingerprint = Some(model_fingerprint);
    state.snapshot_bytes += stored.retained_bytes;
    state.snapshots.push_back(stored);
    Ok(BrowserSnapshotToolOutput::Snapshot(page))
}

fn contains_null(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => true,
        serde_json::Value::Array(values) => values.iter().any(contains_null),
        serde_json::Value::Object(values) => values.values().any(contains_null),
        _ => false,
    }
}

fn map_browser_error(error: BrowserRequestError) -> DomainError {
    match error {
        BrowserRequestError::Domain(error) => public_browser_error(error),
        BrowserRequestError::Infrastructure(_) => DomainError::new(
            "INTERNAL_ERROR",
            "The browser snapshot request could not be completed.",
        ),
    }
}

fn public_browser_error(error: DomainError) -> DomainError {
    let message = match error.code.as_str() {
        "CAPABILITY_DISABLED" => "The required browser capability is disabled.",
        "CAPABILITY_UNAVAILABLE" => "The required browser capability is unavailable.",
        "NOT_FOUND" => "The requested browser object no longer exists.",
        "RATE_LIMITED" => "Chrome temporarily rate-limited the browser request.",
        "INVALID_ARGUMENT" => "The extension rejected the browser snapshot request.",
        "TIMEOUT" | "DEADLINE_EXCEEDED" => "The browser snapshot request timed out.",
        "RESULT_TOO_LARGE" | "RESPONSE_TOO_LARGE" => {
            "The browser snapshot is too large; narrow the request."
        }
        _ => "The browser snapshot request failed.",
    };
    let code = match error.code.as_str() {
        "CAPABILITY_DISABLED"
        | "CAPABILITY_UNAVAILABLE"
        | "NOT_FOUND"
        | "RATE_LIMITED"
        | "INVALID_ARGUMENT"
        | "RESULT_TOO_LARGE" => error.code,
        "TIMEOUT" | "DEADLINE_EXCEEDED" => "TIMEOUT".to_owned(),
        "RESPONSE_TOO_LARGE" => "RESULT_TOO_LARGE".to_owned(),
        _ => "INTERNAL_ERROR".to_owned(),
    };
    DomainError::new(code, message)
}

fn matched_tabs(
    baseline: &Baseline,
    references: &SnapshotReferences,
    params: &BrowserSnapshotParams,
) -> Result<Vec<String>, DomainError> {
    let window_key = params
        .window_ref
        .as_ref()
        .map(|reference| resolve_ref(&references.windows, &reference.0, "window"))
        .transpose()?;
    let group_key = params
        .group_ref
        .as_ref()
        .map(|reference| resolve_ref(&references.groups, &reference.0, "group"))
        .transpose()?;
    let tab_keys = params
        .tab_refs
        .as_ref()
        .map(|requested| {
            requested
                .iter()
                .map(|reference| resolve_ref(&references.tabs, &reference.0, "tab"))
                .collect::<Result<HashSet<_>, _>>()
        })
        .transpose()?;
    let query = params.query.as_ref().map(|query| query.to_lowercase());

    let windows: HashMap<&str, &BaselineWindow> = baseline
        .windows
        .iter()
        .map(|window| (window.key.as_str(), window))
        .collect();
    let mut tabs: Vec<&BaselineTab> = baseline
        .tabs
        .iter()
        .filter(|tab| {
            window_key.as_ref().is_none_or(|key| key == &tab.window_key)
                && group_key
                    .as_ref()
                    .is_none_or(|key| tab.group_key.as_ref() == Some(key))
                && tab_keys
                    .as_ref()
                    .is_none_or(|keys| keys.contains(tab.key.as_str()))
                && params.active.is_none_or(|value| tab.active == value)
                && params.pinned.is_none_or(|value| tab.pinned == value)
                && params.discarded.is_none_or(|value| tab.discarded == value)
                && params.frozen.is_none_or(|value| tab.frozen == Some(value))
                && query.as_ref().is_none_or(|query| {
                    tab.title
                        .as_ref()
                        .is_some_and(|title| title.to_lowercase().contains(query))
                        || tab
                            .url
                            .as_ref()
                            .is_some_and(|url| url.to_lowercase().contains(query))
                        || tab
                            .pending_url
                            .as_ref()
                            .is_some_and(|url| url.to_lowercase().contains(query))
                })
        })
        .collect();
    tabs.sort_by(|left, right| {
        let left_window = windows[left.window_key.as_str()];
        let right_window = windows[right.window_key.as_str()];
        right_window
            .focused
            .cmp(&left_window.focused)
            .then_with(|| left_window.id.cmp(&right_window.id))
            .then_with(|| left.index.cmp(&right.index))
    });
    Ok(tabs.into_iter().map(|tab| tab.key.clone()).collect())
}

fn resolve_ref(
    references: &HashMap<String, String>,
    requested: &str,
    object_type: &str,
) -> Result<String, DomainError> {
    references
        .iter()
        .find_map(|(key, reference)| (reference == requested).then(|| key.clone()))
        .ok_or_else(|| {
            DomainError::new(
                "NOT_FOUND",
                format!("The referenced {object_type} no longer exists."),
            )
        })
}

fn counts(baseline: &Baseline, matched: &[String]) -> SnapshotCounts {
    let matched: HashSet<&str> = matched.iter().map(String::as_str).collect();
    let tabs: Vec<&BaselineTab> = baseline
        .tabs
        .iter()
        .filter(|tab| matched.contains(tab.key.as_str()))
        .collect();
    SnapshotCounts {
        window_count: tabs
            .iter()
            .map(|tab| tab.window_key.as_str())
            .collect::<HashSet<_>>()
            .len(),
        group_count: tabs
            .iter()
            .filter_map(|tab| tab.group_key.as_deref())
            .collect::<HashSet<_>>()
            .len(),
        tab_count: tabs.len(),
        discarded_tab_count: tabs.iter().filter(|tab| tab.discarded).count(),
    }
}

fn retained_size(snapshot: &StoredSnapshot) -> Result<usize, DomainError> {
    let baseline_bytes = serde_json::to_vec(&snapshot.baseline)
        .map_err(|_| invalid_baseline("The browser snapshot could not be measured."))?
        .len();
    let reference_bytes = serde_json::to_vec(&snapshot.references)
        .map_err(|_| invalid_baseline("The browser references could not be measured."))?
        .len();
    let matched_bytes = snapshot
        .matched_tab_keys
        .iter()
        .map(String::len)
        .sum::<usize>();
    let cursor_bytes = snapshot
        .cursors
        .keys()
        .map(String::len)
        .sum::<usize>()
        .saturating_add(snapshot.cursors.len() * std::mem::size_of::<usize>())
        .saturating_add(
            snapshot
                .cursor_by_offset
                .values()
                .map(String::len)
                .sum::<usize>(),
        )
        .saturating_add(snapshot.cursor_by_offset.len() * std::mem::size_of::<usize>());
    Ok(baseline_bytes
        .saturating_add(reference_bytes)
        .saturating_add(matched_bytes)
        .saturating_add(cursor_bytes))
}

fn prepare_cursors(snapshot: &mut StoredSnapshot) {
    let mut offset = snapshot.limit;
    while offset < snapshot.matched_tab_keys.len() {
        let cursor = random_ref("cur");
        snapshot.cursors.insert(cursor.clone(), offset);
        snapshot.cursor_by_offset.insert(offset, cursor);
        offset = offset.saturating_add(snapshot.limit);
    }
}

fn continue_snapshot(runtime: &BrokerRuntime, cursor: &str) -> Result<SnapshotPage, DomainError> {
    let mut state = runtime.state();
    remove_expired(&mut state);
    for snapshot in &mut state.snapshots {
        if let Some(offset) = snapshot.cursors.get(cursor).copied() {
            return render_page(snapshot, offset);
        }
    }
    Err(DomainError::new(
        "HANDLE_EXPIRED",
        "The browser snapshot cursor is invalid or no longer retained.",
    ))
}

fn remove_expired(state: &mut crate::runtime::RuntimeState) {
    while state
        .snapshots
        .front()
        .is_some_and(|snapshot| snapshot.created_at.elapsed() >= SNAPSHOT_TTL)
    {
        evict_oldest(state);
    }
}

fn evict_oldest(state: &mut crate::runtime::RuntimeState) {
    if let Some(snapshot) = state.snapshots.pop_front() {
        state.snapshot_bytes = state.snapshot_bytes.saturating_sub(snapshot.retained_bytes);
    }
}

fn render_page(snapshot: &mut StoredSnapshot, offset: usize) -> Result<SnapshotPage, DomainError> {
    let end = offset
        .saturating_add(snapshot.limit)
        .min(snapshot.matched_tab_keys.len());
    let page_keys = &snapshot.matched_tab_keys[offset..end];
    let tab_by_key: HashMap<&str, &BaselineTab> = snapshot
        .baseline
        .tabs
        .iter()
        .map(|tab| (tab.key.as_str(), tab))
        .collect();
    let window_by_key: HashMap<&str, &BaselineWindow> = snapshot
        .baseline
        .windows
        .iter()
        .map(|window| (window.key.as_str(), window))
        .collect();
    let group_by_key: HashMap<&str, &BaselineGroup> = snapshot
        .baseline
        .groups
        .iter()
        .map(|group| (group.key.as_str(), group))
        .collect();
    let all_matching: HashSet<&str> = snapshot
        .matched_tab_keys
        .iter()
        .map(String::as_str)
        .collect();
    let page_set: HashSet<&str> = page_keys.iter().map(String::as_str).collect();

    let mut windows = Vec::new();
    let mut current_window_key: Option<&str> = None;
    for key in page_keys {
        let tab = tab_by_key[key.as_str()];
        if current_window_key != Some(tab.window_key.as_str()) {
            current_window_key = Some(&tab.window_key);
            let window = window_by_key[tab.window_key.as_str()];
            let matching_in_window = snapshot
                .baseline
                .tabs
                .iter()
                .filter(|candidate| {
                    candidate.window_key == tab.window_key
                        && all_matching.contains(candidate.key.as_str())
                })
                .count();
            let page_in_window = page_keys
                .iter()
                .filter(|key| tab_by_key[key.as_str()].window_key == tab.window_key)
                .count();
            windows.push(SnapshotWindow {
                reference: snapshot.references.windows[window.key.as_str()].clone(),
                focused: window.focused,
                items: Vec::new(),
                partial: page_in_window < matching_in_window,
                top: full(snapshot.detail, window.top),
                left: full(snapshot.detail, window.left),
                width: full(snapshot.detail, window.width),
                height: full(snapshot.detail, window.height),
                window_type: full(snapshot.detail, window.window_type.clone()),
                state: full(snapshot.detail, window.state.clone()),
                always_on_top: full(snapshot.detail, window.always_on_top),
            });
        }

        let output_tab = project_tab(tab, snapshot.detail, &snapshot.references);
        let window = windows.last_mut().ok_or_else(|| {
            invalid_baseline("The browser snapshot hierarchy could not be reconstructed.")
        })?;
        if let Some(group_key) = tab.group_key.as_deref() {
            if let Some(SnapshotItem::Group(previous)) = window.items.last_mut()
                && previous.group.reference == snapshot.references.groups[group_key]
            {
                previous.tabs.push(output_tab);
                continue;
            }
            let group = group_by_key[group_key];
            let matching_in_group = snapshot
                .baseline
                .tabs
                .iter()
                .filter(|candidate| {
                    candidate.group_key.as_deref() == Some(group_key)
                        && all_matching.contains(candidate.key.as_str())
                })
                .count();
            let page_in_group = snapshot
                .baseline
                .tabs
                .iter()
                .filter(|candidate| {
                    candidate.group_key.as_deref() == Some(group_key)
                        && page_set.contains(candidate.key.as_str())
                })
                .count();
            window.items.push(SnapshotItem::Group(SnapshotGroupItem {
                group: SnapshotGroup {
                    reference: snapshot.references.groups[group_key].clone(),
                    title: group.title.clone().filter(|title| !title.is_empty()),
                    color: group.color.clone(),
                    partial: page_in_group < matching_in_group,
                    collapsed: full(snapshot.detail, Some(group.collapsed)),
                    shared: full(snapshot.detail, group.shared),
                },
                tabs: vec![output_tab],
            }));
        } else {
            window.items.push(SnapshotItem::Tab(output_tab));
        }
    }

    let next_cursor = if end < snapshot.matched_tab_keys.len() {
        snapshot.cursor_by_offset.get(&end).cloned()
    } else {
        None
    };
    let page = SnapshotPage {
        browser_snapshot_ref: snapshot.snapshot_ref.clone(),
        captured_at: snapshot.baseline.captured_at.clone(),
        total_matched: snapshot.matched_tab_keys.len(),
        windows,
        next_cursor,
    };
    let size = serde_json::to_vec(&page)
        .map_err(|_| invalid_baseline("The browser snapshot page could not be serialized."))?
        .len();
    if size > MAX_RESULT_BYTES {
        return Err(DomainError::new(
            "RESULT_TOO_LARGE",
            "The browser snapshot page is too large; reduce limit or use compact detail.",
        ));
    }
    Ok(page)
}

fn full<T>(detail: SnapshotDetail, value: Option<T>) -> Option<T> {
    (detail == SnapshotDetail::Full).then_some(value).flatten()
}

fn project_tab(
    tab: &BaselineTab,
    detail: SnapshotDetail,
    references: &SnapshotReferences,
) -> SnapshotTab {
    let is_full = detail == SnapshotDetail::Full;
    SnapshotTab {
        reference: references.tabs[tab.key.as_str()].clone(),
        title: tab.title.clone().filter(|title| !title.is_empty()),
        url: tab.url.clone().filter(|url| !url.is_empty()),
        active: tab.active,
        pinned: tab.pinned,
        discarded: tab.discarded,
        frozen: if is_full {
            tab.frozen
        } else {
            tab.frozen.filter(|frozen| *frozen)
        },
        pending_url: is_full
            .then(|| tab.pending_url.clone().filter(|url| !url.is_empty()))
            .flatten(),
        highlighted: is_full.then_some(tab.highlighted),
        audible: is_full.then_some(tab.audible).flatten(),
        muted: is_full.then_some(tab.muted).flatten(),
        loading: is_full
            .then(|| tab.status.as_deref().map(|status| status == "loading"))
            .flatten(),
        auto_discardable: is_full.then_some(tab.auto_discardable).flatten(),
        last_accessed: is_full.then_some(tab.last_accessed).flatten(),
        opener_ref: is_full
            .then(|| {
                tab.opener_key
                    .as_ref()
                    .and_then(|key| references.tabs.get(key).cloned())
            })
            .flatten(),
        fav_icon_url: is_full
            .then(|| tab.fav_icon_url.clone().filter(|url| !url.is_empty()))
            .flatten(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Baseline, BrowserSnapshotParams, SnapshotDetail, decode_params};

    #[test]
    fn baseline_rejects_unknown_fields_and_duplicate_indexes() {
        let value = json!({
            "browserInstanceId": "browser",
            "modelRevision": 1,
            "capturedAt": "2026-08-03T12:00:00Z",
            "supportsFrozenTabs": true,
            "supportsSharedTabGroups": false,
            "windows": [{"key":"window-1","id":1,"focused":true}],
            "groups": [],
            "tabs": [
                {"key":"tab-1","id":1,"windowKey":"window-1","index":0,"active":true,"highlighted":true,"pinned":false,"discarded":false},
                {"key":"tab-2","id":2,"windowKey":"window-1","index":0,"active":false,"highlighted":false,"pinned":false,"discarded":false}
            ]
        });
        let baseline: Baseline = serde_json::from_value(value).unwrap();
        assert!(baseline.validate("browser").is_err());
    }

    #[test]
    fn counts_rejects_an_explicit_limit() {
        let params = BrowserSnapshotParams {
            detail: Some(SnapshotDetail::Counts),
            limit: Some(1),
            ..Default::default()
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn strict_decoder_rejects_unknown_fields_and_explicit_null() {
        let unknown = decode_params(json!({"unknown": true})).unwrap_err();
        assert_eq!(unknown.code, "INVALID_ARGUMENT");

        let explicit_null = decode_params(json!({"cursor": null})).unwrap_err();
        assert_eq!(explicit_null.code, "INVALID_ARGUMENT");
    }

    #[test]
    fn baseline_support_flags_require_complete_consistent_values() {
        let missing_frozen = json!({
            "browserInstanceId": "browser",
            "modelRevision": 1,
            "capturedAt": "2026-08-03T12:00:00Z",
            "supportsFrozenTabs": true,
            "supportsSharedTabGroups": false,
            "windows": [{"key":"window-1","id":1,"focused":true}],
            "groups": [],
            "tabs": [{
                "key":"tab-1","id":1,"windowKey":"window-1","index":0,
                "active":true,"highlighted":true,"pinned":false,"discarded":false
            }]
        });
        let baseline: Baseline = serde_json::from_value(missing_frozen).unwrap();
        assert!(baseline.validate("browser").is_err());
    }
}
