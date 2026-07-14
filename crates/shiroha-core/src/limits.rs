use std::time::Duration;

use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CpuBudget {
    Epoch { ticks: u64 },
    Fuel { units: u64 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvocationLimits {
    pub cpu_budget: CpuBudget,
    pub wall_time: Duration,
    pub max_memory_bytes: usize,
    pub max_table_elements: usize,
    pub max_instances: usize,
    pub max_tables: usize,
    pub max_memories: usize,
}

impl Default for InvocationLimits {
    fn default() -> Self {
        Self {
            cpu_budget: CpuBudget::Epoch { ticks: 100 },
            wall_time: Duration::from_secs(1),
            max_memory_bytes: 64 * 1024 * 1024,
            max_table_elements: 10_000,
            max_instances: 16,
            max_tables: 16,
            max_memories: 16,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeLimits {
    pub invocation: InvocationLimits,
    pub max_payload_bytes: usize,
    pub max_metadata_bytes: usize,
    pub max_events_per_hook: usize,
    pub max_microsteps: usize,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            invocation: InvocationLimits::default(),
            max_payload_bytes: 1024 * 1024,
            max_metadata_bytes: 4 * 1024,
            max_events_per_hook: 256,
            max_microsteps: 1_024,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadLimits {
    pub max_artifact_bytes: usize,
    pub max_states: usize,
    pub max_transitions: usize,
    pub max_functions: usize,
}

impl Default for LoadLimits {
    fn default() -> Self {
        Self {
            max_artifact_bytes: 16 * 1024 * 1024,
            max_states: 1_024,
            max_transitions: 8_192,
            max_functions: 4_096,
        }
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("runtime limit `{field}` must be finite and greater than zero")]
pub struct LimitsError {
    pub field: &'static str,
}

impl InvocationLimits {
    pub fn validate(&self) -> Result<(), LimitsError> {
        match self.cpu_budget {
            CpuBudget::Epoch { ticks: 0 } => {
                return Err(LimitsError {
                    field: "epoch_ticks",
                });
            }
            CpuBudget::Fuel { units: 0 } => {
                return Err(LimitsError {
                    field: "fuel_units",
                });
            }
            CpuBudget::Epoch { .. } | CpuBudget::Fuel { .. } => {}
        }

        if self.wall_time.is_zero() {
            return Err(LimitsError { field: "wall_time" });
        }
        ensure_nonzero(self.max_memory_bytes, "max_memory_bytes")?;
        ensure_nonzero(self.max_table_elements, "max_table_elements")?;
        ensure_nonzero(self.max_instances, "max_instances")?;
        ensure_nonzero(self.max_tables, "max_tables")?;
        ensure_nonzero(self.max_memories, "max_memories")
    }
}

impl RuntimeLimits {
    pub fn validate(&self) -> Result<(), LimitsError> {
        self.invocation.validate()?;
        ensure_nonzero(self.max_payload_bytes, "max_payload_bytes")?;
        ensure_nonzero(self.max_metadata_bytes, "max_metadata_bytes")?;
        ensure_nonzero(self.max_events_per_hook, "max_events_per_hook")?;
        ensure_nonzero(self.max_microsteps, "max_microsteps")
    }
}

impl LoadLimits {
    pub fn validate(&self) -> Result<(), LimitsError> {
        ensure_nonzero(self.max_artifact_bytes, "max_artifact_bytes")?;
        ensure_nonzero(self.max_states, "max_states")?;
        ensure_nonzero(self.max_transitions, "max_transitions")?;
        ensure_nonzero(self.max_functions, "max_functions")
    }
}

fn ensure_nonzero(value: usize, field: &'static str) -> Result<(), LimitsError> {
    if value == 0 {
        Err(LimitsError { field })
    } else {
        Ok(())
    }
}
