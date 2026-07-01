//! State machine runtime: state tree, transitions, RTC event loop, history.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use shiroha_ir::{
    ActionRef, EventId, GuardRef, HistoryConfig, State, StateId, StateMachineDef, Transition,
};
use tokio::sync::{RwLock, mpsc};
use tokio::task::JoinHandle;

use crate::{
    ActionContext, ActionInvoker, ActionResult, Event, GuardEvaluator, TaskHandle, TaskId,
    TaskState,
};

/// State tree representation for efficient navigation.
#[derive(Debug)]
struct StateTree {
    states: HashMap<StateId, Arc<State>>,
    #[allow(dead_code)]
    children: HashMap<StateId, Vec<StateId>>,
}

impl StateTree {
    fn new(states: Vec<State>) -> Self {
        let states_map: HashMap<StateId, Arc<State>> = states
            .into_iter()
            .map(|s| (s.id.clone(), Arc::new(s)))
            .collect();

        let mut children: HashMap<StateId, Vec<StateId>> = HashMap::new();

        for state in states_map.values() {
            if let Some(parent) = &state.parent {
                children
                    .entry(parent.clone())
                    .or_default()
                    .push(state.id.clone());
            }
        }

        Self {
            states: states_map,
            children,
        }
    }

    fn get_state(&self, id: &StateId) -> Option<&Arc<State>> {
        self.states.get(id)
    }

    #[allow(dead_code)]
    fn get_children(&self, id: &StateId) -> Vec<StateId> {
        self.children.get(id).cloned().unwrap_or_default()
    }

    /// Compute the path from state to root.
    fn path_to_root(&self, state_id: &StateId) -> Vec<StateId> {
        let mut path = vec![state_id.clone()];
        let mut current = state_id.clone();

        while let Some(state) = self.get_state(&current) {
            if let Some(parent) = &state.parent {
                path.push(parent.clone());
                current = parent.clone();
            } else {
                break;
            }
        }

        path
    }

    /// Compute Lowest Common Ancestor (LCA) for transition path calculation.
    fn lca(&self, from: &StateId, to: &StateId) -> Option<StateId> {
        let from_path = self.path_to_root(from);
        let to_path = self.path_to_root(to);

        let from_set: HashSet<_> = from_path.iter().collect();

        for ancestor in to_path.iter() {
            if from_set.contains(ancestor) {
                return Some(ancestor.clone());
            }
        }

        None
    }
}

/// History storage for shallow and deep history.
#[derive(Debug, Default)]
struct HistoryStore {
    /// Map from parent state to last active child (shallow history).
    shallow: HashMap<StateId, StateId>,
    /// Map from parent state to full active path (deep history).
    deep: HashMap<StateId, Vec<StateId>>,
}

impl HistoryStore {
    fn record_shallow(&mut self, parent: &StateId, child: &StateId) {
        self.shallow.insert(parent.clone(), child.clone());
    }

    fn record_deep(&mut self, parent: &StateId, path: Vec<StateId>) {
        self.deep.insert(parent.clone(), path);
    }

    #[allow(dead_code)]
    fn get_shallow(&self, parent: &StateId) -> Option<&StateId> {
        self.shallow.get(parent)
    }

    #[allow(dead_code)]
    fn get_deep(&self, parent: &StateId) -> Option<&Vec<StateId>> {
        self.deep.get(parent)
    }
}

/// Configuration for a state machine instance.
#[derive(Debug)]
struct Configuration {
    current_state: StateId,
    history: HistoryStore,
}

/// Task actor running the state machine.
pub struct Task {
    id: TaskId,
    def: Arc<StateMachineDef>,
    tree: Arc<StateTree>,
    config: Arc<RwLock<Configuration>>,
    action_invoker: Arc<dyn ActionInvoker>,
    guard_evaluator: Arc<dyn GuardEvaluator>,
    receiver: mpsc::UnboundedReceiver<Event>,
    do_activity_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
    state: Arc<RwLock<TaskState>>,
}

impl Task {
    /// Create a new task instance.
    pub fn new(
        id: TaskId,
        def: StateMachineDef,
        action_invoker: Arc<dyn ActionInvoker>,
        guard_evaluator: Arc<dyn GuardEvaluator>,
        component_path: Option<std::path::PathBuf>,
    ) -> (Self, TaskHandle) {
        let (sender, receiver) = mpsc::unbounded_channel();

        let tree = Arc::new(StateTree::new(def.states.clone()));
        let config = Arc::new(RwLock::new(Configuration {
            current_state: def.initial.clone(),
            history: HistoryStore::default(),
        }));

        let state = Arc::new(RwLock::new(TaskState {
            task_id: id.clone(),
            current_state: def.initial.clone(),
            active_do_activity: None,
        }));

        let task = Self {
            id: id.clone(),
            def: Arc::new(def),
            tree,
            config,
            action_invoker,
            guard_evaluator,
            receiver,
            do_activity_handle: Arc::new(RwLock::new(None)),
            state: state.clone(),
        };

        let handle = TaskHandle::new(id, sender, state, component_path);

        (task, handle)
    }

    /// Run the task (spawn as tokio task).
    pub fn run(mut self) -> JoinHandle<()> {
        tokio::spawn(async move {
            // Enter initial state
            let initial_state = self.def.initial.clone();
            self.enter_state(&initial_state).await;
            self.update_shared_state().await;

            // RTC event loop
            while let Some(event) = self.receiver.recv().await {
                self.process_event(event).await;
                self.update_shared_state().await;
            }
        })
    }

    /// Update the shared state after transitions.
    async fn update_shared_state(&self) {
        let mut state = self.state.write().await;
        let config = self.config.read().await;
        let do_activity = self.do_activity_handle.read().await;

        state.current_state = config.current_state.clone();
        state.active_do_activity = if do_activity.is_some() {
            Some("active".to_string())
        } else {
            None
        };
    }

    /// Get current task state.
    pub async fn get_state(&self) -> TaskState {
        let config = self.config.read().await;
        let do_activity = self.do_activity_handle.read().await;

        TaskState {
            task_id: self.id.clone(),
            current_state: config.current_state.clone(),
            active_do_activity: if do_activity.is_some() {
                Some("active".to_string())
            } else {
                None
            },
        }
    }

    /// Process a single event (RTC: run-to-completion).
    async fn process_event(&mut self, event: Event) {
        let config = self.config.read().await;
        let current = config.current_state.clone();
        drop(config);

        // Find matching transition
        let matching_transition = self.find_matching_transition(&current, &event.name).await;

        if let Some(transition) = matching_transition {
            // Execute transition
            self.execute_transition(transition, &event).await;
        }
    }

    /// Find a matching transition from the current state for the given event.
    async fn find_matching_transition(
        &self,
        current: &StateId,
        event: &EventId,
    ) -> Option<Arc<Transition>> {
        // Find all transitions from current state with matching event
        let candidates: Vec<_> = self
            .def
            .transitions
            .iter()
            .filter(|t| t.from == *current && t.event == *event)
            .collect();

        // Evaluate guards and return first matching
        for transition in candidates {
            if self.evaluate_guard(&transition.guard).await {
                return Some(Arc::new(transition.clone()));
            }
        }

        None
    }

    /// Evaluate a guard condition.
    async fn evaluate_guard(&self, guard: &Option<GuardRef>) -> bool {
        match guard {
            None | Some(GuardRef::Always) => true,
            Some(GuardRef::Wasm(name)) | Some(GuardRef::Plugin(name)) => {
                let ctx = ActionContext {
                    task_id: self.id.clone(),
                    event: None,
                    payload: None,
                };
                self.guard_evaluator
                    .evaluate(name, &ctx)
                    .await
                    .unwrap_or(false)
            }
        }
    }

    /// Execute a transition (exit -> action -> enter).
    async fn execute_transition(&mut self, transition: Arc<Transition>, event: &Event) {
        let from = transition.from.clone();
        let to = transition.to.clone();

        // Calculate exit/entry sequence using LCA
        let exit_sequence = self.compute_exit_sequence(&from, &to);
        let entry_sequence = self.compute_entry_sequence(&from, &to);

        // Exit states
        for state_id in exit_sequence {
            self.exit_state(&state_id).await;
        }

        // Execute transition action
        if let Some(action) = &transition.action {
            self.invoke_action_sync(action, event).await;
        }

        // Enter states
        for state_id in entry_sequence {
            self.enter_state(&state_id).await;
        }

        // Update current state
        let mut config = self.config.write().await;
        config.current_state = to.clone();
    }

    /// Compute the sequence of states to exit.
    fn compute_exit_sequence(&self, from: &StateId, to: &StateId) -> Vec<StateId> {
        let lca = self.tree.lca(from, to);
        let from_path = self.tree.path_to_root(from);

        let lca_idx = if let Some(lca_state) = &lca {
            from_path.iter().position(|s| s == lca_state)
        } else {
            None
        };

        if let Some(idx) = lca_idx {
            from_path[..idx].to_vec()
        } else {
            from_path
        }
    }

    /// Compute the sequence of states to enter.
    fn compute_entry_sequence(&self, from: &StateId, to: &StateId) -> Vec<StateId> {
        let lca = self.tree.lca(from, to);
        let to_path = self.tree.path_to_root(to);

        let lca_idx = if let Some(lca_state) = &lca {
            to_path.iter().position(|s| s == lca_state)
        } else {
            None
        };

        let mut entry = if let Some(idx) = lca_idx {
            to_path[..idx].to_vec()
        } else {
            to_path
        };

        entry.reverse();
        entry
    }

    /// Enter a state (execute entry action and start do-activity).
    async fn enter_state(&mut self, state_id: &StateId) {
        let state = self.tree.get_state(state_id).cloned();

        if let Some(state) = state {
            // Execute entry action
            if let Some(action) = &state.entry {
                let event = Event {
                    name: "entry".to_string(),
                    payload: None,
                };
                self.invoke_action_sync(action, &event).await;
            }

            // Start do-activity if present
            if let Some(do_action) = &state.do_activity {
                self.start_do_activity(do_action).await;
            }

            // Record history for parent if configured
            if let Some(parent) = &state.parent
                && let Some(parent_state) = self.tree.get_state(parent)
            {
                let mut config = self.config.write().await;
                match parent_state.history {
                    HistoryConfig::Shallow => {
                        config.history.record_shallow(parent, state_id);
                    }
                    HistoryConfig::Deep => {
                        let path = self.tree.path_to_root(state_id);
                        config.history.record_deep(parent, path);
                    }
                    HistoryConfig::None => {}
                }
            }
        }
    }

    /// Exit a state (cancel do-activity and execute exit action).
    async fn exit_state(&mut self, state_id: &StateId) {
        // Cancel do-activity if running
        self.cancel_do_activity().await;

        if let Some(state) = self.tree.get_state(state_id) {
            // Execute exit action
            if let Some(action) = &state.exit {
                let event = Event {
                    name: "exit".to_string(),
                    payload: None,
                };
                self.invoke_action_sync(action, &event).await;
            }
        }
    }

    /// Invoke a synchronous action.
    async fn invoke_action_sync(&self, action: &ActionRef, event: &Event) {
        let ctx = ActionContext {
            task_id: self.id.clone(),
            event: Some(event.name.clone()),
            payload: event.payload.clone(),
        };

        match self.action_invoker.invoke_sync(&action.name, ctx).await {
            Ok(ActionResult::Signal(signal)) => {
                // TODO: Inject signal as internal event
                let _ = signal;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("Action {} failed: {}", action.name, e);
            }
        }
    }

    /// Start a do-activity (spawn as separate task).
    async fn start_do_activity(&mut self, action: &ActionRef) {
        let ctx = ActionContext {
            task_id: self.id.clone(),
            event: None,
            payload: None,
        };

        let invoker = self.action_invoker.clone();
        let action_name = action.name.clone();

        let handle = tokio::spawn(async move {
            match invoker.invoke_do(&action_name, ctx).await {
                Ok(ActionResult::Signal(signal)) => {
                    // TODO: Send completion event to task
                    let _ = signal;
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Do-activity {} failed: {}", action_name, e);
                }
            }
        });

        let mut do_handle = self.do_activity_handle.write().await;
        *do_handle = Some(handle);
    }

    /// Cancel the current do-activity if running.
    async fn cancel_do_activity(&mut self) {
        let mut do_handle = self.do_activity_handle.write().await;
        if let Some(handle) = do_handle.take() {
            handle.abort();
        }
    }
}

/// Task manager for controlling task lifecycle.
#[derive(Clone)]
pub struct TaskManager {
    tasks: Arc<RwLock<HashMap<TaskId, TaskHandle>>>,
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new task instance and start it.
    pub async fn create_task(
        &self,
        id: TaskId,
        def: StateMachineDef,
        action_invoker: Arc<dyn ActionInvoker>,
        guard_evaluator: Arc<dyn GuardEvaluator>,
        component_path: Option<std::path::PathBuf>,
    ) -> anyhow::Result<TaskHandle> {
        let (task, handle) = Task::new(
            id.clone(),
            def,
            action_invoker,
            guard_evaluator,
            component_path,
        );

        task.run();

        let mut tasks = self.tasks.write().await;
        tasks.insert(id, handle.clone());

        Ok(handle)
    }

    /// Get a task handle by ID.
    pub async fn get_task(&self, id: &TaskId) -> Option<TaskHandle> {
        let tasks = self.tasks.read().await;
        tasks.get(id).cloned()
    }

    /// List all task IDs.
    pub async fn list_tasks(&self) -> Vec<TaskId> {
        let tasks = self.tasks.read().await;
        tasks.keys().cloned().collect()
    }
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}
