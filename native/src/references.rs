use std::{borrow::Borrow, collections::HashMap, hash::Hash};

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::protocol::DomainError;

macro_rules! reference_type {
    ($name:ident, $prefix:literal) => {
        #[derive(Clone, Debug, Deserialize, Eq, Hash, JsonSchema, PartialEq, Serialize)]
        #[serde(transparent)]
        pub(crate) struct $name(String);

        impl $name {
            pub(crate) fn issue() -> Self {
                Self(format!("{}_{}", $prefix, Uuid::new_v4().simple()))
            }

            pub(crate) fn validate(&self, field: &str) -> Result<(), DomainError> {
                if self.0.len() <= $prefix.len() + 1 || !self.0.starts_with(concat!($prefix, "_")) {
                    return Err(DomainError::invalid_argument(format!(
                        "{field} is not a valid {} reference.",
                        $prefix
                    )));
                }
                Ok(())
            }

            pub(crate) fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Borrow<str> for $name {
            fn borrow(&self) -> &str {
                self.as_str()
            }
        }
    };
}

reference_type!(WindowRef, "win");
reference_type!(GroupRef, "grp");
reference_type!(TabRef, "tab");
reference_type!(BrowserSnapshotRef, "bs");
reference_type!(CursorRef, "cur");

impl BrowserSnapshotRef {
    pub(crate) fn parse(field: &str, value: &str) -> Result<Self, DomainError> {
        let reference = Self(value.to_owned());
        reference.validate(field)?;
        Ok(reference)
    }
}

pub(crate) trait BrowserObjectRef: Clone + Eq + Hash {
    fn issue() -> Self;
}

impl BrowserObjectRef for WindowRef {
    fn issue() -> Self {
        Self::issue()
    }
}

impl BrowserObjectRef for GroupRef {
    fn issue() -> Self {
        Self::issue()
    }
}

impl BrowserObjectRef for TabRef {
    fn issue() -> Self {
        Self::issue()
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SnapshotReferences {
    windows: HashMap<String, WindowRef>,
    groups: HashMap<String, GroupRef>,
    tabs: HashMap<String, TabRef>,
}

impl SnapshotReferences {
    pub(crate) fn new(
        windows: HashMap<String, WindowRef>,
        groups: HashMap<String, GroupRef>,
        tabs: HashMap<String, TabRef>,
    ) -> Self {
        Self {
            windows,
            groups,
            tabs,
        }
    }

    pub(crate) fn window_for_key(&self, key: &str) -> Option<&WindowRef> {
        self.windows.get(key)
    }

    pub(crate) fn group_for_key(&self, key: &str) -> Option<&GroupRef> {
        self.groups.get(key)
    }

    pub(crate) fn tab_for_key(&self, key: &str) -> Option<&TabRef> {
        self.tabs.get(key)
    }

    pub(crate) fn resolve_window(&self, reference: &WindowRef) -> Result<String, DomainError> {
        resolve_ref(&self.windows, reference, "window")
    }

    pub(crate) fn resolve_group(&self, reference: &GroupRef) -> Result<String, DomainError> {
        resolve_ref(&self.groups, reference, "group")
    }

    pub(crate) fn resolve_tabs(
        &self,
        requested: &[TabRef],
    ) -> Result<std::collections::HashSet<String>, DomainError> {
        requested
            .iter()
            .map(|reference| resolve_ref(&self.tabs, reference, "tab"))
            .collect()
    }

    pub(crate) fn retained_bytes(&self) -> Result<usize, DomainError> {
        serde_json::to_vec(self)
            .map(|encoded| encoded.len())
            .map_err(|_| {
                DomainError::new(
                    "INTERNAL_ERROR",
                    "The browser references could not be measured.",
                )
            })
    }
}

fn resolve_ref<R: PartialEq>(
    references: &HashMap<String, R>,
    requested: &R,
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

#[cfg(test)]
mod tests {
    use super::{BrowserSnapshotRef, CursorRef, TabRef, WindowRef};

    #[test]
    fn issued_references_are_typed_and_transparently_serialized() {
        let window = WindowRef::issue();
        assert!(window.as_str().starts_with("win_"));
        assert_eq!(serde_json::to_value(&window).unwrap(), window.as_str());
        let wrong_kind: TabRef =
            serde_json::from_value(serde_json::json!(window.as_str())).unwrap();
        assert!(wrong_kind.validate("tabRef").is_err());

        let snapshot = BrowserSnapshotRef::issue();
        let cursor = CursorRef::issue();
        assert!(snapshot.as_str().starts_with("bs_"));
        assert!(cursor.as_str().starts_with("cur_"));
    }
}
