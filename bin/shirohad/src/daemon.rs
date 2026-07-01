//! Daemon structure and component loading logic.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use shiroha_engine::{ActionInvoker, Adapter, GuardEvaluator, TaskManager};
use shiroha_wasm::{Engine, WasmActionInvoker, WasmAdapter};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

/// Daemon managing multiple tasks and control interfaces.
pub struct Daemon {
    /// Task manager (internally Arc, cheap to clone).
    pub task_manager: TaskManager,
    /// Task ID -> component path mapping.
    pub component_paths: Arc<RwLock<HashMap<String, PathBuf>>>,
    /// Unix socket path.
    pub socket_path: PathBuf,
    /// Global cancellation token (used for shutdown coordination).
    #[allow(dead_code)]
    pub cancel_token: CancellationToken,
}

impl Daemon {
    /// Create a new daemon instance.
    pub fn new(socket_path: PathBuf, cancel_token: CancellationToken) -> Self {
        Self {
            task_manager: TaskManager::new(),
            component_paths: Arc::new(RwLock::new(HashMap::new())),
            socket_path,
            cancel_token,
        }
    }

    /// Load multiple components and create tasks.
    pub async fn load_components(&self, paths: &[PathBuf]) -> Result<usize> {
        let engine = self.create_wasm_engine()?;
        let mut success_count = 0;

        for path in paths {
            match self.load_single_component(&engine, path).await {
                Ok(task_id) => {
                    info!("Task created: id={}, component={}", task_id, path.display());
                    success_count += 1;
                }
                Err(e) => {
                    error!("Failed to load component {}: {}", path.display(), e);
                    // Skip and continue loading remaining components
                }
            }
        }

        if success_count == 0 {
            anyhow::bail!("All components failed to load");
        }

        Ok(success_count)
    }

    /// Load a single component and create a task.
    async fn load_single_component(&self, engine: &Arc<Engine>, path: &Path) -> Result<String> {
        let task_id = generate_task_id(path);

        info!("Loading component: {}", path.display());

        // Load state machine definition
        let adapter = WasmAdapter::from_file(engine.clone(), path)
            .context("Failed to create WASM adapter")?;

        let def = adapter
            .load()
            .await
            .context("Failed to load state machine definition")?;

        info!(
            "Component loaded: {} states, {} transitions, {} events",
            def.states.len(),
            def.transitions.len(),
            def.events.len()
        );
        info!("Initial state: {}", def.initial);

        // Create action invoker
        let action_invoker: Arc<dyn ActionInvoker> =
            Arc::new(WasmActionInvoker::from_file(engine.clone(), path)?);

        // Create guard evaluator
        let guard_evaluator: Arc<dyn GuardEvaluator> = Arc::new(NoopGuardEvaluator);

        // Create task
        let _handle = self
            .task_manager
            .create_task(
                task_id.clone(),
                def,
                action_invoker,
                guard_evaluator,
                Some(path.to_path_buf()),
            )
            .await
            .context("Failed to create task")?;

        // Record component path
        self.component_paths
            .write()
            .await
            .insert(task_id.clone(), path.to_path_buf());

        Ok(task_id)
    }

    /// Create WASM engine with component model enabled.
    fn create_wasm_engine(&self) -> Result<Arc<Engine>> {
        let mut config = wasmtime::Config::new();
        config.wasm_component_model(true);
        Ok(Arc::new(Engine::new(&config)?))
    }
}

/// Generate a task ID from component path.
fn generate_task_id(component_path: &Path) -> String {
    let name = component_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("task");
    let uuid = uuid::Uuid::new_v4();
    format!("{}-{}", name, &uuid.to_string()[..8])
}

/// Placeholder guard evaluator (always returns true).
struct NoopGuardEvaluator;

#[async_trait::async_trait]
impl GuardEvaluator for NoopGuardEvaluator {
    async fn evaluate(
        &self,
        _guard: &str,
        _ctx: &shiroha_engine::ActionContext,
    ) -> anyhow::Result<bool> {
        Ok(true)
    }
}
