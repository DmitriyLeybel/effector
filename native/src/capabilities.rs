use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use rmcp::{
    RoleServer,
    service::{Peer, SubscriptionContext},
};
use tokio::sync::mpsc;

use crate::protocol::{Capabilities, CapabilityStatus, DomainError, ImplementationEntry};

const IMPLEMENTATION_ABI_REVISION: u32 = 1;

#[derive(Clone, Debug, Default)]
pub(crate) struct DiscoverySnapshot {
    capability_revision: Option<u64>,
    capabilities: Option<Capabilities>,
    visible_tools: Arc<HashSet<&'static str>>,
}

#[derive(Clone, Copy)]
pub(crate) enum CapabilityKind {
    BrowserSnapshot,
}

#[derive(Clone, Copy)]
struct ToolDescriptor {
    name: &'static str,
    branch: Option<&'static str>,
    abi_revision: u32,
    capability: CapabilityKind,
}

const TOOL_DESCRIPTORS: [ToolDescriptor; 3] = [
    ToolDescriptor {
        name: "browser.list",
        branch: None,
        abi_revision: IMPLEMENTATION_ABI_REVISION,
        capability: CapabilityKind::BrowserSnapshot,
    },
    ToolDescriptor {
        name: "browser.snapshot",
        branch: None,
        abi_revision: IMPLEMENTATION_ABI_REVISION,
        capability: CapabilityKind::BrowserSnapshot,
    },
    ToolDescriptor {
        name: "tabs.list",
        branch: None,
        abi_revision: IMPLEMENTATION_ABI_REVISION,
        capability: CapabilityKind::BrowserSnapshot,
    },
];

impl DiscoverySnapshot {
    pub(crate) fn connected(
        capability_revision: u64,
        capabilities: &Capabilities,
        implementations: &[ImplementationEntry],
    ) -> Self {
        let visible_tools = TOOL_DESCRIPTORS
            .iter()
            .filter(|descriptor| {
                capability_status(capabilities, descriptor.capability).effective
                    && implementations.iter().any(|implementation| {
                        implementation.method == descriptor.name
                            && implementation.branch.as_deref() == descriptor.branch
                            && implementation.abi_revision == descriptor.abi_revision
                    })
            })
            .map(|descriptor| descriptor.name)
            .collect();
        Self {
            capability_revision: Some(capability_revision),
            capabilities: Some(capabilities.clone()),
            visible_tools: Arc::new(visible_tools),
        }
    }

    pub(crate) fn allows(&self, tool_name: &str) -> bool {
        self.visible_tools.contains(tool_name)
    }

    pub(crate) fn is_ready(&self) -> bool {
        self.capability_revision.is_some()
    }

    pub(crate) fn require_capability(&self, kind: CapabilityKind) -> Result<(), DomainError> {
        let capabilities = self.capabilities.as_ref().ok_or_else(|| {
            DomainError::new(
                "CAPABILITY_UNAVAILABLE",
                "The Chrome extension handshake is not complete.",
            )
        })?;
        let label = match kind {
            CapabilityKind::BrowserSnapshot => "Browser snapshots",
        };
        capability_result(capability_status(capabilities, kind), label)
    }
}

fn capability_status(capabilities: &Capabilities, kind: CapabilityKind) -> &CapabilityStatus {
    match kind {
        CapabilityKind::BrowserSnapshot => &capabilities.browser_snapshot,
    }
}

fn capability_result(status: &CapabilityStatus, label: &str) -> Result<(), DomainError> {
    if status.effective {
        return Ok(());
    }
    if status.reason == "disabled" {
        return Err(DomainError::new(
            "CAPABILITY_DISABLED",
            format!("{label} are disabled in Effector."),
        ));
    }
    Err(DomainError::new(
        "CAPABILITY_UNAVAILABLE",
        format!("{label} are unavailable in the connected Chrome build."),
    ))
}

pub(crate) struct ToolListNotifier {
    sender: mpsc::Sender<()>,
    receiver: Mutex<Option<mpsc::Receiver<()>>>,
}

impl ToolListNotifier {
    pub(crate) fn new() -> Arc<Self> {
        let (sender, receiver) = mpsc::channel(1);
        Arc::new(Self {
            sender,
            receiver: Mutex::new(Some(receiver)),
        })
    }

    fn take_receiver(&self) -> Option<mpsc::Receiver<()>> {
        self.receiver
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }

    pub(crate) fn start_legacy(&self, peer: Peer<RoleServer>) {
        let Some(mut receiver) = self.take_receiver() else {
            return;
        };
        tokio::spawn(async move {
            while receiver.recv().await.is_some() {
                if peer.notify_tool_list_changed().await.is_err() {
                    break;
                }
            }
        });
    }

    pub(crate) async fn listen(&self, context: SubscriptionContext) {
        let Some(mut receiver) = self.take_receiver() else {
            context.cancelled().await;
            return;
        };
        loop {
            tokio::select! {
                _ = context.cancelled() => return,
                notification = receiver.recv() => {
                    if notification.is_none()
                        || context.sink().notify_tool_list_changed().await.is_err()
                    {
                        return;
                    }
                }
            }
        }
    }

    pub(crate) fn notify(&self) {
        let _ = self.sender.try_send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capabilities(browser_snapshot: bool) -> Capabilities {
        let status = |effective| CapabilityStatus {
            implemented: effective,
            desired: effective,
            granted: effective,
            supported: effective,
            probe_passed: effective,
            effective,
            reason: if effective {
                "available".to_owned()
            } else {
                "notImplemented".to_owned()
            },
        };
        Capabilities {
            browser_snapshot: status(browser_snapshot),
            browser_change: status(false),
            page_tools: status(false),
            advanced_evaluation: status(false),
            frozen_tabs: true,
            shared_tab_groups: true,
        }
    }

    fn implementation(
        method: &str,
        branch: Option<&str>,
        abi_revision: u32,
    ) -> ImplementationEntry {
        ImplementationEntry {
            method: method.to_owned(),
            branch: branch.map(str::to_owned),
            abi_revision,
        }
    }

    #[test]
    fn discovery_is_empty_before_ready() {
        let snapshot = DiscoverySnapshot::default();

        assert!(!snapshot.is_ready());
        assert!(!snapshot.allows("browser.snapshot"));
    }

    #[test]
    fn discovery_intersects_exact_implementation_keys() {
        let snapshot = DiscoverySnapshot::connected(
            7,
            &capabilities(true),
            &[
                implementation("browser.list", None, 1),
                implementation("browser.snapshot", Some("preview"), 1),
                implementation("tabs.list", None, 2),
            ],
        );

        assert!(snapshot.is_ready());
        assert!(snapshot.allows("browser.list"));
        assert!(!snapshot.allows("browser.snapshot"));
        assert!(!snapshot.allows("tabs.list"));
    }

    #[test]
    fn discovery_requires_the_effective_tool_capability() {
        let snapshot = DiscoverySnapshot::connected(
            7,
            &capabilities(false),
            &[
                implementation("browser.list", None, 1),
                implementation("browser.snapshot", None, 1),
                implementation("tabs.list", None, 1),
            ],
        );

        assert!(snapshot.is_ready());
        assert!(!snapshot.allows("browser.list"));
        assert!(!snapshot.allows("browser.snapshot"));
        assert!(!snapshot.allows("tabs.list"));
    }

    #[test]
    fn capability_errors_do_not_expose_extension_detail() {
        let unavailable = DiscoverySnapshot::connected(1, &capabilities(false), &[]);

        let error = unavailable
            .require_capability(CapabilityKind::BrowserSnapshot)
            .unwrap_err();
        assert_eq!(error.code, "CAPABILITY_UNAVAILABLE");
        assert_eq!(
            error.message,
            "Browser snapshots are unavailable in the connected Chrome build."
        );
    }
}
