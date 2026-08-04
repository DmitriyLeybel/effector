use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex, MutexGuard},
};

use tokio::{
    sync::watch,
    task::{AbortHandle, JoinHandle},
};
use uuid::Uuid;

const MAX_ADMITTED_BROWSER_OPERATIONS: usize = 32;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct OperationKey(Uuid);

impl OperationKey {
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "operation admission begins with browser.change apply"
        )
    )]
    pub(crate) fn issue() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(
    not(test),
    allow(dead_code, reason = "operation lanes begin with browser.change apply")
)]
pub(crate) enum BrowserLane {
    Browser,
    Window(String),
    Group(String),
    Tab(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BrowserLaneSet(Arc<[BrowserLane]>);

impl BrowserLaneSet {
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "operation lanes begin with browser.change apply")
    )]
    pub(crate) fn new(mut lanes: Vec<BrowserLane>) -> Result<Self, OperationAdmissionError> {
        if lanes.is_empty() {
            return Err(OperationAdmissionError::EmptyLanes);
        }
        lanes.sort_unstable();
        lanes.dedup();
        Ok(Self(lanes.into()))
    }

    fn overlaps(&self, other: &Self) -> bool {
        self.0.contains(&BrowserLane::Browser)
            || other.0.contains(&BrowserLane::Browser)
            || self.0.iter().any(|lane| other.0.contains(lane))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OperationTerminal {
    Completed,
    NotDispatched,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TaskStatus {
    Queued,
    Running,
    Completed,
    NotDispatched,
    Unknown,
}

impl TaskStatus {
    fn terminal(self) -> Option<OperationTerminal> {
        match self {
            Self::Completed => Some(OperationTerminal::Completed),
            Self::NotDispatched => Some(OperationTerminal::NotDispatched),
            Self::Unknown => Some(OperationTerminal::Unknown),
            Self::Queued | Self::Running => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OperationAdmissionError {
    Closed,
    Saturated,
    KeyConflict,
    EmptyLanes,
}

struct TaskEntry {
    sequence: u64,
    lanes: BrowserLaneSet,
    status: watch::Sender<TaskStatus>,
    abort: Option<AbortHandle>,
    owner_retained: bool,
}

#[derive(Default)]
struct RegistryState {
    closed: bool,
    next_sequence: u64,
    tasks: HashMap<OperationKey, TaskEntry>,
}

#[derive(Clone, Default)]
pub(crate) struct OperationTasks {
    inner: Arc<Mutex<RegistryState>>,
}

pub(crate) struct OperationJoin {
    status: watch::Receiver<TaskStatus>,
}

impl OperationTasks {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "operation admission begins with browser.change apply"
        )
    )]
    pub(crate) fn join_or_spawn<F, Fut>(
        &self,
        key: OperationKey,
        lanes: BrowserLaneSet,
        run: F,
    ) -> Result<OperationJoin, OperationAdmissionError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = OperationTerminal> + Send + 'static,
    {
        let (status, predecessors) = {
            let mut state = self.state();
            if let Some(task) = state.tasks.get(&key) {
                if task.lanes != lanes {
                    return Err(OperationAdmissionError::KeyConflict);
                }
                return Ok(OperationJoin {
                    status: task.status.subscribe(),
                });
            }
            if state.closed {
                return Err(OperationAdmissionError::Closed);
            }
            if state.tasks.len() >= MAX_ADMITTED_BROWSER_OPERATIONS
                || state.next_sequence == u64::MAX
            {
                return Err(OperationAdmissionError::Saturated);
            }

            let mut predecessors = state
                .tasks
                .values()
                .filter(|task| {
                    task.lanes.overlaps(&lanes)
                        && !matches!(
                            *task.status.borrow(),
                            TaskStatus::Completed | TaskStatus::NotDispatched
                        )
                })
                .map(|task| (task.sequence, task.status.subscribe()))
                .collect::<Vec<_>>();
            predecessors.sort_unstable_by_key(|(sequence, _)| *sequence);

            let sequence = state.next_sequence;
            state.next_sequence += 1;
            let (status, receiver) = watch::channel(TaskStatus::Queued);
            state.tasks.insert(
                key.clone(),
                TaskEntry {
                    sequence,
                    lanes,
                    status: status.clone(),
                    abort: None,
                    owner_retained: true,
                },
            );
            (status, (receiver, predecessors))
        };

        let (receiver, predecessors) = predecessors;
        let tasks = self.clone();
        let task_key = key.clone();
        let task_status = status.clone();
        let handle = tokio::spawn(async move {
            let mut terminal_on_drop =
                TerminalOnDrop::new(tasks.clone(), task_key.clone(), task_status.clone());
            for (_, mut predecessor) in predecessors {
                if wait_for_terminal(&mut predecessor).await == OperationTerminal::Unknown {
                    tasks.finish(&task_key, &task_status, OperationTerminal::NotDispatched);
                    terminal_on_drop.disarm();
                    return;
                }
            }
            if !transition(&task_status, TaskStatus::Queued, TaskStatus::Running) {
                terminal_on_drop.disarm();
                return;
            }
            let terminal = run().await;
            tasks.finish(&task_key, &task_status, terminal);
            terminal_on_drop.disarm();
        });
        self.attach_abort(&key, &status, handle);

        Ok(OperationJoin { status: receiver })
    }

    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "operation eviction begins with retained plans")
    )]
    pub(crate) fn evict_owner(&self, key: &OperationKey) {
        let mut state = self.state();
        let remove = state.tasks.get_mut(key).is_some_and(|task| {
            task.owner_retained = false;
            task.status.borrow().terminal().is_some()
        });
        if remove {
            state.tasks.remove(key);
        }
    }

    pub(crate) fn shutdown_and_clear(&self) {
        let aborts = {
            let mut state = self.state();
            state.closed = true;
            let aborts = state
                .tasks
                .values_mut()
                .filter_map(|task| {
                    fail_unfinished(&task.status);
                    task.abort.take()
                })
                .collect::<Vec<_>>();
            state.tasks.clear();
            aborts
        };
        for abort in aborts {
            abort.abort();
        }
    }

    fn attach_abort(
        &self,
        key: &OperationKey,
        status: &watch::Sender<TaskStatus>,
        handle: JoinHandle<()>,
    ) {
        let abort = handle.abort_handle();
        drop(handle);
        let mut state = self.state();
        if let Some(task) = state
            .tasks
            .get_mut(key)
            .filter(|task| task.status.same_channel(status))
        {
            if task.status.borrow().terminal().is_none() {
                task.abort = Some(abort);
            }
        } else if state.closed {
            abort.abort();
        }
    }

    fn finish(
        &self,
        key: &OperationKey,
        status: &watch::Sender<TaskStatus>,
        terminal: OperationTerminal,
    ) {
        terminal_transition(status, terminal);
        let mut state = self.state();
        let remove = state
            .tasks
            .get_mut(key)
            .filter(|task| task.status.same_channel(status))
            .is_some_and(|task| {
                task.abort = None;
                !task.owner_retained
            });
        if remove {
            state.tasks.remove(key);
        }
    }

    fn state(&self) -> MutexGuard<'_, RegistryState> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl OperationJoin {
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "operation waiters begin with browser.change apply"
        )
    )]
    pub(crate) async fn wait(&mut self) -> OperationTerminal {
        wait_for_terminal(&mut self.status).await
    }
}

struct TerminalOnDrop {
    tasks: OperationTasks,
    key: OperationKey,
    status: watch::Sender<TaskStatus>,
    armed: bool,
}

impl TerminalOnDrop {
    fn new(tasks: OperationTasks, key: OperationKey, status: watch::Sender<TaskStatus>) -> Self {
        Self {
            tasks,
            key,
            status,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TerminalOnDrop {
    fn drop(&mut self) {
        if self.armed {
            let terminal = match *self.status.borrow() {
                TaskStatus::Queued => OperationTerminal::NotDispatched,
                TaskStatus::Running => OperationTerminal::Unknown,
                TaskStatus::Completed | TaskStatus::NotDispatched | TaskStatus::Unknown => return,
            };
            self.tasks.finish(&self.key, &self.status, terminal);
        }
    }
}

fn transition(status: &watch::Sender<TaskStatus>, from: TaskStatus, to: TaskStatus) -> bool {
    status.send_if_modified(|current| {
        if *current == from {
            *current = to;
            true
        } else {
            false
        }
    })
}

fn terminal_transition(status: &watch::Sender<TaskStatus>, terminal: OperationTerminal) -> bool {
    let next = match terminal {
        OperationTerminal::Completed => TaskStatus::Completed,
        OperationTerminal::NotDispatched => TaskStatus::NotDispatched,
        OperationTerminal::Unknown => TaskStatus::Unknown,
    };
    status.send_if_modified(|current| {
        if current.terminal().is_none() {
            *current = next;
            true
        } else {
            false
        }
    })
}

fn fail_unfinished(status: &watch::Sender<TaskStatus>) {
    status.send_if_modified(|current| match *current {
        TaskStatus::Queued => {
            *current = TaskStatus::NotDispatched;
            true
        }
        TaskStatus::Running => {
            *current = TaskStatus::Unknown;
            true
        }
        TaskStatus::Completed | TaskStatus::NotDispatched | TaskStatus::Unknown => false,
    });
}

async fn wait_for_terminal(status: &mut watch::Receiver<TaskStatus>) -> OperationTerminal {
    loop {
        if let Some(terminal) = status.borrow_and_update().terminal() {
            return terminal;
        }
        if status.changed().await.is_err() {
            return match *status.borrow() {
                TaskStatus::Queued => OperationTerminal::NotDispatched,
                TaskStatus::Running => OperationTerminal::Unknown,
                TaskStatus::Completed => OperationTerminal::Completed,
                TaskStatus::NotDispatched => OperationTerminal::NotDispatched,
                TaskStatus::Unknown => OperationTerminal::Unknown,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use tokio::sync::oneshot;

    use super::{
        BrowserLane, BrowserLaneSet, MAX_ADMITTED_BROWSER_OPERATIONS, OperationAdmissionError,
        OperationKey, OperationTasks, OperationTerminal, terminal_transition,
    };

    fn lanes(lanes: Vec<BrowserLane>) -> BrowserLaneSet {
        BrowserLaneSet::new(lanes).unwrap()
    }

    #[test]
    fn browser_lane_overlaps_every_specific_lane() {
        let browser = lanes(vec![BrowserLane::Browser]);
        let tab = lanes(vec![BrowserLane::Tab("tab".to_owned())]);
        assert!(browser.overlaps(&tab));
        assert!(tab.overlaps(&browser));
    }

    #[tokio::test]
    async fn same_key_spawns_once_and_shares_late_completion() {
        let tasks = OperationTasks::new();
        let key = OperationKey::issue();
        let lanes = lanes(vec![BrowserLane::Tab("tab-a".to_owned())]);
        let starts = Arc::new(AtomicUsize::new(0));
        let task_starts = starts.clone();
        let (release_tx, release_rx) = oneshot::channel();
        let first = tasks
            .join_or_spawn(key.clone(), lanes.clone(), move || async move {
                task_starts.fetch_add(1, Ordering::SeqCst);
                let _ = release_rx.await;
                OperationTerminal::Completed
            })
            .unwrap();
        let mut second = tasks
            .join_or_spawn(key.clone(), lanes.clone(), || async {
                panic!("joined operation must not spawn twice")
            })
            .unwrap();
        drop(first);

        release_tx.send(()).unwrap();
        assert_eq!(second.wait().await, OperationTerminal::Completed);
        assert_eq!(starts.load(Ordering::SeqCst), 1);

        let mut late = tasks
            .join_or_spawn(key, lanes, || async {
                panic!("completed operation must remain deduplicated")
            })
            .unwrap();
        assert_eq!(late.wait().await, OperationTerminal::Completed);
    }

    #[tokio::test]
    async fn capacity_counts_retained_tasks_but_allows_join() {
        let tasks = OperationTasks::new();
        let mut retained = Vec::new();
        for index in 0..MAX_ADMITTED_BROWSER_OPERATIONS {
            let key = OperationKey::issue();
            let lanes = lanes(vec![BrowserLane::Tab(format!("tab-{index}"))]);
            let mut waiter = tasks
                .join_or_spawn(key.clone(), lanes.clone(), || async {
                    OperationTerminal::Completed
                })
                .unwrap();
            assert_eq!(waiter.wait().await, OperationTerminal::Completed);
            retained.push((key, lanes));
        }

        assert!(
            tasks
                .join_or_spawn(retained[0].0.clone(), retained[0].1.clone(), || async {
                    panic!("duplicate admission must join")
                })
                .is_ok()
        );
        assert_eq!(
            tasks
                .join_or_spawn(
                    OperationKey::issue(),
                    lanes(vec![BrowserLane::Browser]),
                    || async { OperationTerminal::Completed },
                )
                .err(),
            Some(OperationAdmissionError::Saturated)
        );

        tasks.evict_owner(&retained[0].0);
        assert!(
            tasks
                .join_or_spawn(
                    OperationKey::issue(),
                    lanes(vec![BrowserLane::Window("window".to_owned())]),
                    || async { OperationTerminal::Completed },
                )
                .is_ok()
        );
    }

    #[tokio::test]
    async fn overlapping_tasks_are_ordered_while_disjoint_tasks_run() {
        let tasks = OperationTasks::new();
        let lane_a = lanes(vec![BrowserLane::Tab("tab-a".to_owned())]);
        let lane_b = lanes(vec![BrowserLane::Tab("tab-b".to_owned())]);
        let (a_started_tx, a_started_rx) = oneshot::channel();
        let (a_release_tx, a_release_rx) = oneshot::channel();
        let mut first = tasks
            .join_or_spawn(OperationKey::issue(), lane_a.clone(), move || async move {
                let _ = a_started_tx.send(());
                let _ = a_release_rx.await;
                OperationTerminal::Completed
            })
            .unwrap();
        let (overlap_started_tx, mut overlap_started_rx) = oneshot::channel();
        let mut overlap = tasks
            .join_or_spawn(OperationKey::issue(), lane_a, move || async move {
                let _ = overlap_started_tx.send(());
                OperationTerminal::Completed
            })
            .unwrap();
        let (disjoint_started_tx, disjoint_started_rx) = oneshot::channel();
        let mut disjoint = tasks
            .join_or_spawn(OperationKey::issue(), lane_b, move || async move {
                let _ = disjoint_started_tx.send(());
                OperationTerminal::Completed
            })
            .unwrap();

        a_started_rx.await.unwrap();
        disjoint_started_rx.await.unwrap();
        assert!(matches!(
            overlap_started_rx.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        a_release_tx.send(()).unwrap();
        assert_eq!(first.wait().await, OperationTerminal::Completed);
        assert_eq!(overlap.wait().await, OperationTerminal::Completed);
        assert_eq!(disjoint.wait().await, OperationTerminal::Completed);
    }

    #[tokio::test]
    async fn shutdown_distinguishes_running_from_queued_tasks() {
        let tasks = OperationTasks::new();
        let lane_set = lanes(vec![BrowserLane::Group("group".to_owned())]);
        let (started_tx, started_rx) = oneshot::channel();
        let (_release_tx, release_rx) = oneshot::channel::<()>();
        let mut running = tasks
            .join_or_spawn(
                OperationKey::issue(),
                lane_set.clone(),
                move || async move {
                    let _ = started_tx.send(());
                    let _ = release_rx.await;
                    OperationTerminal::Completed
                },
            )
            .unwrap();
        let mut queued = tasks
            .join_or_spawn(OperationKey::issue(), lane_set, || async {
                panic!("overlapping queued operation must not run after shutdown")
            })
            .unwrap();
        started_rx.await.unwrap();

        tasks.shutdown_and_clear();
        assert_eq!(running.wait().await, OperationTerminal::Unknown);
        assert_eq!(queued.wait().await, OperationTerminal::NotDispatched);
        assert_eq!(
            tasks
                .join_or_spawn(
                    OperationKey::issue(),
                    lanes(vec![BrowserLane::Browser]),
                    || async { OperationTerminal::Completed },
                )
                .err(),
            Some(OperationAdmissionError::Closed)
        );
    }

    #[tokio::test]
    async fn one_key_cannot_change_its_overlap_lanes() {
        let tasks = OperationTasks::new();
        let key = OperationKey::issue();
        tasks
            .join_or_spawn(
                key.clone(),
                lanes(vec![BrowserLane::Tab("tab-a".to_owned())]),
                || async { OperationTerminal::Completed },
            )
            .unwrap();
        assert_eq!(
            tasks
                .join_or_spawn(
                    key,
                    lanes(vec![BrowserLane::Tab("tab-b".to_owned())]),
                    || async { OperationTerminal::Completed },
                )
                .err(),
            Some(OperationAdmissionError::KeyConflict)
        );
    }

    #[tokio::test]
    async fn unknown_predecessor_does_not_poison_undispatched_transitive_lanes() {
        let tasks = OperationTasks::new();
        let lane_x = BrowserLane::Tab("tab-x".to_owned());
        let lane_y = BrowserLane::Tab("tab-y".to_owned());
        let mut uncertain = tasks
            .join_or_spawn(
                OperationKey::issue(),
                lanes(vec![lane_x.clone()]),
                || async { OperationTerminal::Unknown },
            )
            .unwrap();
        let mut blocked = tasks
            .join_or_spawn(
                OperationKey::issue(),
                lanes(vec![lane_x, lane_y.clone()]),
                || async { panic!("uncertain predecessor must prevent dispatch") },
            )
            .unwrap();

        assert_eq!(uncertain.wait().await, OperationTerminal::Unknown);
        assert_eq!(blocked.wait().await, OperationTerminal::NotDispatched);

        let mut transitive = tasks
            .join_or_spawn(OperationKey::issue(), lanes(vec![lane_y]), || async {
                OperationTerminal::Completed
            })
            .unwrap();
        assert_eq!(transitive.wait().await, OperationTerminal::Completed);
    }

    #[tokio::test]
    async fn stale_task_callbacks_cannot_modify_a_reused_key() {
        let tasks = OperationTasks::new();
        let key = OperationKey::issue();
        let lanes = lanes(vec![BrowserLane::Tab("tab".to_owned())]);
        tasks
            .join_or_spawn(key.clone(), lanes.clone(), future::pending)
            .unwrap();
        let old_status = tasks.state().tasks[&key].status.clone();
        terminal_transition(&old_status, OperationTerminal::Completed);
        tasks.evict_owner(&key);

        tasks
            .join_or_spawn(key.clone(), lanes, future::pending)
            .unwrap();
        let replacement_abort = tasks.state().tasks[&key].abort.as_ref().unwrap().id();

        tasks.finish(&key, &old_status, OperationTerminal::Completed);
        assert_eq!(
            tasks.state().tasks[&key].abort.as_ref().unwrap().id(),
            replacement_abort
        );

        let stale_handle = tokio::spawn(future::pending());
        tasks.attach_abort(&key, &old_status, stale_handle);
        assert_eq!(
            tasks.state().tasks[&key].abort.as_ref().unwrap().id(),
            replacement_abort
        );
        tasks.shutdown_and_clear();
    }
}
