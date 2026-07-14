use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use shiroha_core::{
    CpuBudget, InvocationLimits, ResourceLimitKind, RuntimeFault, RuntimeFaultKind,
};
use thiserror::Error;
use wasmtime::{Config, Engine, ResourceLimiter, Store, Trap};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use crate::error::{WasmError, diagnostic};

const EPOCH_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BudgetMode {
    Epoch,
    Fuel,
}

pub(crate) struct RuntimeInner {
    pub engine: Engine,
    pub mode: BudgetMode,
    _epoch_ticker: Option<EpochTicker>,
}

impl RuntimeInner {
    pub fn new(limits: &InvocationLimits) -> Result<Arc<Self>, WasmError> {
        limits.validate()?;
        let mut config = Config::new();
        config.wasm_component_model(true);
        let mode = match limits.cpu_budget {
            CpuBudget::Epoch { .. } => {
                config.epoch_interruption(true);
                BudgetMode::Epoch
            }
            CpuBudget::Fuel { .. } => {
                config.consume_fuel(true);
                BudgetMode::Fuel
            }
        };
        let engine = Engine::new(&config).map_err(|error| WasmError::Engine {
            operation: "engine creation",
            diagnostic: diagnostic(&error),
        })?;
        let epoch_ticker = if mode == BudgetMode::Epoch {
            Some(
                EpochTicker::start(engine.clone()).map_err(|error| WasmError::Engine {
                    operation: "epoch ticker creation",
                    diagnostic: error.to_string(),
                })?,
            )
        } else {
            None
        };

        Ok(Arc::new(Self {
            engine,
            mode,
            _epoch_ticker: epoch_ticker,
        }))
    }
}

struct EpochTicker {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl EpochTicker {
    fn start(engine: Engine) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let handle = thread::Builder::new()
            .name("shiroha-wasmtime-epoch".to_owned())
            .spawn(move || {
                while !thread_stop.load(Ordering::Acquire) {
                    thread::sleep(EPOCH_INTERVAL);
                    engine.increment_epoch();
                }
            })?;
        Ok(Self {
            stop,
            handle: Some(handle),
        })
    }
}

impl Drop for EpochTicker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[derive(Debug, Error)]
enum StoreLimitError {
    #[error("linear memory limit exceeded")]
    Memory,
    #[error("table element limit exceeded")]
    Table,
}

pub(crate) struct StoreLimiter {
    max_memory_bytes: usize,
    allocated_memory_bytes: usize,
    table_elements: usize,
    instances: usize,
    tables: usize,
    memories: usize,
}

impl StoreLimiter {
    fn new(limits: &InvocationLimits) -> Self {
        Self {
            max_memory_bytes: limits.max_memory_bytes,
            allocated_memory_bytes: 0,
            table_elements: limits.max_table_elements,
            instances: limits.max_instances,
            tables: limits.max_tables,
            memories: limits.max_memories,
        }
    }
}

impl ResourceLimiter for StoreLimiter {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        let additional = desired.saturating_sub(current);
        if additional
            > self
                .max_memory_bytes
                .saturating_sub(self.allocated_memory_bytes)
            || maximum.is_some_and(|maximum| desired > maximum)
        {
            Err(wasmtime::Error::new(StoreLimitError::Memory))
        } else {
            self.allocated_memory_bytes += additional;
            Ok(true)
        }
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if desired > self.table_elements || maximum.is_some_and(|maximum| desired > maximum) {
            Err(wasmtime::Error::new(StoreLimitError::Table))
        } else {
            Ok(true)
        }
    }

    fn instances(&self) -> usize {
        self.instances
    }

    fn tables(&self) -> usize {
        self.tables
    }

    fn memories(&self) -> usize {
        self.memories
    }
}

pub(crate) struct StoreState {
    wasi: WasiCtx,
    table: ResourceTable,
    limiter: StoreLimiter,
}

impl StoreState {
    fn new(limits: &InvocationLimits) -> Self {
        Self {
            wasi: WasiCtxBuilder::new().build(),
            table: ResourceTable::new(),
            limiter: StoreLimiter::new(limits),
        }
    }
}

impl WasiView for StoreState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

pub(crate) fn create_store(
    runtime: &RuntimeInner,
    limits: &InvocationLimits,
) -> Result<Store<StoreState>, RuntimeFault> {
    let expected = match limits.cpu_budget {
        CpuBudget::Epoch { .. } => BudgetMode::Epoch,
        CpuBudget::Fuel { .. } => BudgetMode::Fuel,
    };
    if runtime.mode != expected {
        return Err(RuntimeFault::new(
            RuntimeFaultKind::Host,
            "invocation CPU budget mode does not match the Wasmtime Engine",
        ));
    }

    let mut store = Store::new(&runtime.engine, StoreState::new(limits));
    store.limiter(|state| &mut state.limiter);
    reset_budget(&mut store, limits)?;
    Ok(store)
}

pub(crate) fn reset_budget(
    store: &mut Store<StoreState>,
    limits: &InvocationLimits,
) -> Result<(), RuntimeFault> {
    match limits.cpu_budget {
        CpuBudget::Epoch { ticks } => {
            store.set_epoch_deadline(ticks.min(epoch_ticks_before_wall_time(limits.wall_time)));
            Ok(())
        }
        CpuBudget::Fuel { units } => store.set_fuel(units).map_err(classify_wasmtime_error),
    }
}

fn epoch_ticks_before_wall_time(wall_time: Duration) -> u64 {
    let whole_ticks = wall_time.as_nanos() / EPOCH_INTERVAL.as_nanos();
    let before_deadline = whole_ticks.saturating_sub(1).max(1);
    u64::try_from(before_deadline).unwrap_or(u64::MAX)
}

pub(crate) fn classify_wasmtime_error(error: wasmtime::Error) -> RuntimeFault {
    let kind = if matches!(
        error.downcast_ref::<StoreLimitError>(),
        Some(StoreLimitError::Memory)
    ) {
        RuntimeFaultKind::ResourceLimit(ResourceLimitKind::Memory)
    } else if matches!(
        error.downcast_ref::<StoreLimitError>(),
        Some(StoreLimitError::Table)
    ) {
        RuntimeFaultKind::ResourceLimit(ResourceLimitKind::RuntimeResources)
    } else if matches!(error.downcast_ref::<Trap>(), Some(Trap::OutOfFuel)) {
        RuntimeFaultKind::ResourceLimit(ResourceLimitKind::Cpu)
    } else if matches!(error.downcast_ref::<Trap>(), Some(Trap::Interrupt)) {
        RuntimeFaultKind::ResourceLimit(ResourceLimitKind::Deadline)
    } else if error.downcast_ref::<Trap>().is_some() {
        RuntimeFaultKind::Trap
    } else {
        RuntimeFaultKind::Engine
    };
    RuntimeFault::new(kind, diagnostic(&error))
}

pub(crate) fn deadline_fault() -> RuntimeFault {
    RuntimeFault::new(
        RuntimeFaultKind::ResourceLimit(ResourceLimitKind::Deadline),
        "guest call exceeded its wall-time deadline",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_limit_is_aggregated_across_store_memories() {
        let limits = InvocationLimits {
            max_memory_bytes: 100,
            ..InvocationLimits::default()
        };
        let mut limiter = StoreLimiter::new(&limits);

        assert!(limiter.memory_growing(0, 60, None).unwrap());
        assert!(limiter.memory_growing(0, 40, None).unwrap());
        assert!(limiter.memory_growing(0, 1, None).is_err());
    }

    #[test]
    fn epoch_budget_is_capped_before_the_wall_time() {
        assert_eq!(epoch_ticks_before_wall_time(Duration::from_secs(1)), 99);
        assert_eq!(epoch_ticks_before_wall_time(Duration::from_millis(50)), 4);
        assert_eq!(epoch_ticks_before_wall_time(Duration::from_millis(5)), 1);
    }
}
