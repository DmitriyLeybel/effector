use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, MutexGuard},
};

use tokio::sync::oneshot;

use crate::protocol::{BrowserMethod, ResponseMessage};

const MAX_TERMINAL_REQUESTS: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RequestPhase {
    Queued,
    Dispatching,
    Dispatched,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerminalState {
    Responded,
    TimedOut,
    AbandonedWaiter,
}

struct ActiveRequest {
    method: BrowserMethod,
    phase: RequestPhase,
    waiter: oneshot::Sender<ResponseMessage>,
}

struct TerminalRequest {
    method: BrowserMethod,
    phase: RequestPhase,
    state: TerminalState,
}

#[derive(Default)]
struct LifecycleRecords {
    active: HashMap<String, ActiveRequest>,
    terminal: HashMap<String, TerminalRequest>,
    terminal_order: VecDeque<String>,
}

pub(crate) struct MatchedResponse {
    pub(crate) method: BrowserMethod,
    pub(crate) waiter: Option<oneshot::Sender<ResponseMessage>>,
}

#[derive(Clone, Default)]
pub(crate) struct RequestLifecycle {
    records: Arc<Mutex<LifecycleRecords>>,
}

impl RequestLifecycle {
    pub(crate) fn register(
        &self,
        request_id: String,
        method: BrowserMethod,
        waiter: oneshot::Sender<ResponseMessage>,
    ) -> Result<(), &'static str> {
        let mut records = self.records();
        if records.active.contains_key(&request_id) || records.terminal.contains_key(&request_id) {
            return Err("browser request ID collision");
        }
        records.active.insert(
            request_id,
            ActiveRequest {
                method,
                phase: RequestPhase::Queued,
                waiter,
            },
        );
        Ok(())
    }

    /// Claims a queued frame for writing. Once claimed, waiter cancellation no
    /// longer prevents dispatch because a partial Native Messaging write is
    /// conservatively treated as committed.
    pub(crate) fn begin_dispatch(&self, request_id: &str) -> bool {
        let mut records = self.records();
        let Some(record) = records.active.get_mut(request_id) else {
            return false;
        };
        if record.phase != RequestPhase::Queued {
            return false;
        }
        record.phase = RequestPhase::Dispatching;
        true
    }

    pub(crate) fn mark_dispatched(&self, request_id: &str) {
        let mut records = self.records();
        if let Some(record) = records.active.get_mut(request_id)
            && record.phase == RequestPhase::Dispatching
        {
            record.phase = RequestPhase::Dispatched;
        } else if let Some(record) = records.terminal.get_mut(request_id)
            && record.phase == RequestPhase::Dispatching
        {
            record.phase = RequestPhase::Dispatched;
        }
    }

    pub(crate) fn take_response(&self, request_id: &str) -> Option<MatchedResponse> {
        let mut records = self.records();
        if let Some(record) = records.active.remove(request_id) {
            let matched = MatchedResponse {
                method: record.method,
                waiter: Some(record.waiter),
            };
            Self::insert_terminal(
                &mut records,
                request_id,
                TerminalRequest {
                    method: record.method,
                    phase: record.phase,
                    state: TerminalState::Responded,
                },
            );
            return Some(matched);
        }
        let record = records.terminal.get(request_id)?;
        debug_assert!(matches!(
            record.state,
            TerminalState::Responded | TerminalState::TimedOut | TerminalState::AbandonedWaiter
        ));
        Some(MatchedResponse {
            method: record.method,
            waiter: None,
        })
    }

    pub(crate) fn finish(&self, request_id: &str, state: TerminalState) {
        debug_assert!(matches!(
            state,
            TerminalState::TimedOut | TerminalState::AbandonedWaiter
        ));
        let mut records = self.records();
        if let Some(record) = records.active.remove(request_id) {
            Self::insert_terminal(
                &mut records,
                request_id,
                TerminalRequest {
                    method: record.method,
                    phase: record.phase,
                    state,
                },
            );
        }
    }

    pub(crate) fn clear(&self) {
        *self.records() = LifecycleRecords::default();
    }

    fn insert_terminal(records: &mut LifecycleRecords, request_id: &str, record: TerminalRequest) {
        records.terminal.insert(request_id.to_owned(), record);
        records.terminal_order.push_back(request_id.to_owned());
        while records.terminal.len() > MAX_TERMINAL_REQUESTS {
            if let Some(oldest) = records.terminal_order.pop_front() {
                records.terminal.remove(&oldest);
            }
        }
    }

    fn records(&self) -> MutexGuard<'_, LifecycleRecords> {
        self.records
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub(crate) struct RequestWaiterGuard {
    lifecycle: RequestLifecycle,
    request_id: String,
    active: bool,
}

impl RequestWaiterGuard {
    pub(crate) fn new(lifecycle: RequestLifecycle, request_id: String) -> Self {
        Self {
            lifecycle,
            request_id,
            active: true,
        }
    }

    pub(crate) fn timed_out(&mut self) {
        if self.active {
            self.lifecycle
                .finish(&self.request_id, TerminalState::TimedOut);
            self.active = false;
        }
    }
}

impl Drop for RequestWaiterGuard {
    fn drop(&mut self) {
        if self.active {
            self.lifecycle
                .finish(&self.request_id, TerminalState::AbandonedWaiter);
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::oneshot;

    use crate::protocol::BrowserMethod;

    use super::{MAX_TERMINAL_REQUESTS, RequestLifecycle, RequestWaiterGuard};

    #[test]
    fn request_ids_cannot_replace_an_existing_or_recent_request() {
        let lifecycle = RequestLifecycle::default();
        let (first, _) = oneshot::channel();
        lifecycle
            .register("request".to_owned(), BrowserMethod::BrowserList, first)
            .unwrap();
        let (second, _) = oneshot::channel();
        assert_eq!(
            lifecycle
                .register("request".to_owned(), BrowserMethod::TabsList, second)
                .unwrap_err(),
            "browser request ID collision"
        );
        assert_eq!(
            lifecycle.take_response("request").unwrap().method,
            BrowserMethod::BrowserList
        );
        let (third, _) = oneshot::channel();
        assert!(
            lifecycle
                .register("request".to_owned(), BrowserMethod::TabsList, third)
                .is_err()
        );
    }

    #[test]
    fn cancellation_before_writer_claim_prevents_dispatch() {
        let lifecycle = RequestLifecycle::default();
        let (waiter, _) = oneshot::channel();
        lifecycle
            .register("request".to_owned(), BrowserMethod::BrowserList, waiter)
            .unwrap();

        drop(RequestWaiterGuard::new(
            lifecycle.clone(),
            "request".to_owned(),
        ));

        assert!(!lifecycle.begin_dispatch("request"));
        let late = lifecycle.take_response("request").unwrap();
        assert_eq!(late.method, BrowserMethod::BrowserList);
        assert!(late.waiter.is_none());
    }

    #[test]
    fn cancellation_after_writer_claim_preserves_the_dispatch_stage() {
        let lifecycle = RequestLifecycle::default();
        let (waiter, _) = oneshot::channel();
        lifecycle
            .register("request".to_owned(), BrowserMethod::BrowserList, waiter)
            .unwrap();
        assert!(lifecycle.begin_dispatch("request"));

        drop(RequestWaiterGuard::new(
            lifecycle.clone(),
            "request".to_owned(),
        ));
        lifecycle.mark_dispatched("request");

        let late = lifecycle.take_response("request").unwrap();
        assert_eq!(late.method, BrowserMethod::BrowserList);
        assert!(late.waiter.is_none());
    }

    #[test]
    fn a_response_can_win_the_race_with_writer_acknowledgement() {
        let lifecycle = RequestLifecycle::default();
        let (waiter, _) = oneshot::channel();
        lifecycle
            .register("request".to_owned(), BrowserMethod::BrowserSnapshot, waiter)
            .unwrap();
        assert!(lifecycle.begin_dispatch("request"));

        let response = lifecycle.take_response("request").unwrap();
        assert_eq!(response.method, BrowserMethod::BrowserSnapshot);
        assert!(response.waiter.is_some());
        lifecycle.mark_dispatched("request");
        assert!(lifecycle.take_response("request").unwrap().waiter.is_none());
    }

    #[test]
    fn terminal_request_tracking_is_bounded() {
        let lifecycle = RequestLifecycle::default();
        for index in 0..=MAX_TERMINAL_REQUESTS {
            let request_id = format!("request-{index}");
            let (waiter, _) = oneshot::channel();
            lifecycle
                .register(request_id.clone(), BrowserMethod::BrowserList, waiter)
                .unwrap();
            drop(RequestWaiterGuard::new(lifecycle.clone(), request_id));
        }

        assert!(lifecycle.take_response("request-0").is_none());
        assert!(
            lifecycle
                .take_response(&format!("request-{MAX_TERMINAL_REQUESTS}"))
                .is_some()
        );
    }
}
