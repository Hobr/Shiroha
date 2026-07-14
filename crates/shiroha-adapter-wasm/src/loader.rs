use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use shiroha_core::{
    ArtifactBytes, DefinitionAdapter, FunctionExecutorFactory, InvocationLimits, LoadLimits,
    MachineDefinition, PreparationWarning, ValidatedMachine,
};
use tokio::time::timeout;
use tracing::{info, instrument};
use wasmtime::component::{Component, Linker};

use crate::bindings::MachineComponentPre;
use crate::convert::machine_from_wit;
use crate::error::{WasmError, diagnostic};
use crate::executor::WasmExecutorFactory;
use crate::runtime::{
    RuntimeInner, StoreState, classify_wasmtime_error, create_store, deadline_fault, reset_budget,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparationMetadata {
    pub imports: Vec<String>,
    pub compilation_time: Duration,
    pub warnings: Vec<PreparationWarning>,
}

#[derive(Clone)]
pub struct WasmMachineLoader {
    runtime: Arc<RuntimeInner>,
    invocation_limits: InvocationLimits,
}

impl WasmMachineLoader {
    pub fn new(invocation_limits: InvocationLimits) -> Result<Self, WasmError> {
        let runtime = RuntimeInner::new(&invocation_limits)?;
        Ok(Self {
            runtime,
            invocation_limits,
        })
    }

    #[instrument(name = "shiroha.prepare", skip_all, fields(artifact_bytes = artifact.len()))]
    pub async fn prepare(
        &self,
        artifact: ArtifactBytes,
        load_limits: &LoadLimits,
    ) -> Result<PreparedWasmMachine, WasmError> {
        load_limits.validate()?;
        if artifact.is_empty() {
            return Err(WasmError::EmptyArtifact);
        }
        if artifact.len() > load_limits.max_artifact_bytes {
            return Err(WasmError::ArtifactTooLarge {
                actual: artifact.len(),
                maximum: load_limits.max_artifact_bytes,
            });
        }

        let started = Instant::now();
        let component =
            Component::new(&self.runtime.engine, artifact.as_slice()).map_err(|error| {
                WasmError::Engine {
                    operation: "component compilation",
                    diagnostic: diagnostic(&error),
                }
            })?;
        let compilation_time = started.elapsed();
        let imports = component
            .component_type()
            .imports(&self.runtime.engine)
            .map(|(name, _)| name.to_owned())
            .collect::<Vec<_>>();
        let unsupported = imports
            .iter()
            .filter(|name| !is_baseline_import(name))
            .cloned()
            .collect::<Vec<_>>();
        if !unsupported.is_empty() {
            return Err(WasmError::UnsupportedImports {
                imports: unsupported,
            });
        }

        let mut linker = Linker::<StoreState>::new(&self.runtime.engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker).map_err(|error| WasmError::Engine {
            operation: "WASI linker creation",
            diagnostic: diagnostic(&error),
        })?;
        let instance_pre = linker
            .instantiate_pre(&component)
            .map_err(|error| WasmError::Link {
                diagnostic: diagnostic(&error),
            })?;
        let pre = MachineComponentPre::new(instance_pre).map_err(|error| {
            WasmError::IncompatibleComponent {
                diagnostic: diagnostic(&error),
            }
        })?;

        let mut store = create_store(&self.runtime, &self.invocation_limits)
            .map_err(WasmError::DefinitionCall)?;
        let bindings = pre
            .instantiate_async(&mut store)
            .await
            .map_err(|error| WasmError::DefinitionCall(classify_wasmtime_error(error)))?;
        reset_budget(&mut store, &self.invocation_limits).map_err(WasmError::DefinitionCall)?;
        let definition_call = bindings
            .shiroha_machine_definition()
            .call_get_machine(&mut store);
        let guest_definition =
            match timeout(self.invocation_limits.wall_time, definition_call).await {
                Ok(Ok(Ok(definition))) => definition,
                Ok(Ok(Err(error))) => {
                    return Err(WasmError::GuestDefinition {
                        code: error.code,
                        message: error.message,
                    });
                }
                Ok(Err(error)) => {
                    return Err(WasmError::DefinitionCall(classify_wasmtime_error(error)));
                }
                Err(_) => return Err(WasmError::DefinitionCall(deadline_fault())),
            };
        let definition = machine_from_wit(guest_definition)?;
        let validated = Arc::new(ValidatedMachine::new(definition, load_limits)?);
        let metadata = PreparationMetadata {
            imports,
            compilation_time,
            warnings: validated.warnings().to_vec(),
        };
        info!(
            machine = %validated.definition().id,
            states = validated.definition().states.len(),
            imports = metadata.imports.len(),
            "WASM machine prepared"
        );

        Ok(PreparedWasmMachine {
            definition: validated,
            factory: Arc::new(WasmExecutorFactory::new(
                Arc::clone(&self.runtime),
                pre,
                self.invocation_limits.clone(),
            )),
            metadata,
        })
    }
}

#[async_trait]
impl DefinitionAdapter for WasmMachineLoader {
    async fn load_definition(
        &self,
        artifact: ArtifactBytes,
        limits: &LoadLimits,
    ) -> Result<MachineDefinition, shiroha_core::AdapterError> {
        self.prepare(artifact, limits)
            .await
            .map(|prepared| prepared.definition.definition().clone())
            .map_err(|error| shiroha_core::AdapterError {
                message: error.to_string(),
            })
    }
}

#[derive(Clone)]
pub struct PreparedWasmMachine {
    definition: Arc<ValidatedMachine>,
    factory: Arc<WasmExecutorFactory>,
    metadata: PreparationMetadata,
}

impl PreparedWasmMachine {
    #[must_use]
    pub fn definition(&self) -> &Arc<ValidatedMachine> {
        &self.definition
    }

    #[must_use]
    pub fn metadata(&self) -> &PreparationMetadata {
        &self.metadata
    }

    #[must_use]
    pub fn executor_factory(&self) -> Arc<dyn FunctionExecutorFactory> {
        self.factory.clone()
    }

    pub async fn create_executor(
        &self,
    ) -> Result<Box<dyn shiroha_core::FunctionExecutor>, shiroha_core::RuntimeFault> {
        self.factory.create().await
    }
}

// Keep this policy in lockstep with the stable interfaces registered by
// `wasmtime_wasi::p2::add_to_linker_async` for the pinned Wasmtime release.
const MAX_SUPPORTED_WASI_PREVIEW2_PATCH: u64 = 12;

const BASELINE_WASI_INTERFACES: &[&str] = &[
    "wasi:cli/environment",
    "wasi:cli/exit",
    "wasi:cli/stderr",
    "wasi:cli/stdin",
    "wasi:cli/stdout",
    "wasi:cli/terminal-input",
    "wasi:cli/terminal-output",
    "wasi:cli/terminal-stderr",
    "wasi:cli/terminal-stdin",
    "wasi:cli/terminal-stdout",
    "wasi:clocks/monotonic-clock",
    "wasi:clocks/wall-clock",
    "wasi:filesystem/preopens",
    "wasi:filesystem/types",
    "wasi:io/error",
    "wasi:io/poll",
    "wasi:io/streams",
    "wasi:random/insecure",
    "wasi:random/insecure-seed",
    "wasi:random/random",
    "wasi:sockets/instance-network",
    "wasi:sockets/ip-name-lookup",
    "wasi:sockets/network",
    "wasi:sockets/tcp",
    "wasi:sockets/tcp-create-socket",
    "wasi:sockets/udp",
    "wasi:sockets/udp-create-socket",
];

fn is_baseline_import(name: &str) -> bool {
    let Some((interface, version)) = name.rsplit_once('@') else {
        return false;
    };
    BASELINE_WASI_INTERFACES.contains(&interface) && is_supported_wasi_preview2_version(version)
}

fn is_supported_wasi_preview2_version(version: &str) -> bool {
    let mut components = version.split('.');
    matches!(
        (
            components.next(),
            components.next(),
            components.next(),
            components.next(),
        ),
        (Some("0"), Some("2"), Some(patch), None)
            if patch
                .parse::<u64>()
                .is_ok_and(|patch| patch <= MAX_SUPPORTED_WASI_PREVIEW2_PATCH)
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use shiroha_core::{
        CpuBudget, Event, EventName, FailureRecord, HostInput, Lifecycle, MachineInstance,
        PayloadEnvelope, ResourceLimitKind, RunOutcome, RunReport, RuntimeFault, RuntimeFaultKind,
        RuntimeLimits, StateId,
    };

    #[test]
    fn import_policy_accepts_only_linked_wasi_preview2_interfaces() {
        assert!(is_baseline_import("wasi:io/poll@0.2.9"));
        assert!(is_baseline_import("wasi:cli/environment@0.2.9"));
        assert!(is_baseline_import("wasi:sockets/tcp@0.2.12"));
        assert!(!is_baseline_import("wasi:cli/not-real@0.2.9"));
        assert!(!is_baseline_import("wasi:clocks/timezone@0.2.12"));
        assert!(!is_baseline_import("wasi:io/poll@0.2.13"));
        assert!(!is_baseline_import("wasi:io/poll"));
        assert!(!is_baseline_import("wasi:probe/clock"));
        assert!(!is_baseline_import("example:host/api"));
    }

    #[tokio::test]
    async fn unsupported_wasi_and_non_wasi_imports_are_rejected_before_linking() {
        let loader = WasmMachineLoader::new(InvocationLimits::default()).unwrap();
        for import in [
            "wasi:cli/not-real@0.2.9",
            "wasi:io/poll@0.2.13",
            "example:host/api",
        ] {
            let artifact = ArtifactBytes::new(
                format!(r#"(component (import "{import}" (instance)))"#).into_bytes(),
            );
            let error = match loader.prepare(artifact, &LoadLimits::default()).await {
                Ok(_) => panic!("unsupported import `{import}` unexpectedly prepared"),
                Err(error) => error,
            };
            assert!(matches!(
                error,
                WasmError::UnsupportedImports { imports }
                    if imports == [import.to_owned()]
            ));
        }
    }

    #[tokio::test]
    async fn incompatible_component_shape_fails_during_preparation() {
        let loader = WasmMachineLoader::new(InvocationLimits::default()).unwrap();
        let error = match loader
            .prepare(
                ArtifactBytes::new(br#"(component)"#.to_vec()),
                &LoadLimits::default(),
            )
            .await
        {
            Ok(_) => panic!("component without Shiroha exports unexpectedly prepared"),
            Err(error) => error,
        };
        assert!(matches!(error, WasmError::IncompatibleComponent { .. }));
    }

    fn example_artifact() -> ArtifactBytes {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/components/wasm32-wasip2/debug/example_machine.wasm");
        let bytes = std::fs::read(&path).unwrap_or_else(|error| {
            panic!(
                "failed to read {}: {error}; run `just build-example` first",
                path.display()
            )
        });
        ArtifactBytes::new(bytes)
    }

    #[tokio::test]
    async fn definition_load_prepares_host_ir_with_baseline_wasi() {
        let invocation = InvocationLimits::default();
        let loader = WasmMachineLoader::new(invocation).unwrap();
        let prepared = loader
            .prepare(example_artifact(), &LoadLimits::default())
            .await
            .unwrap();

        assert_eq!(
            prepared.definition().definition().id.as_str(),
            "example-machine"
        );
        assert_eq!(prepared.definition().definition().states.len(), 4);
        assert!(!prepared.metadata().imports.is_empty());
        assert!(
            prepared
                .metadata()
                .imports
                .iter()
                .all(|import| import.starts_with("wasi:"))
        );
    }

    #[tokio::test]
    async fn guest_calls_complete_the_example_machine() {
        let limits = RuntimeLimits::default();
        let loader = WasmMachineLoader::new(limits.invocation.clone()).unwrap();
        let prepared = loader
            .prepare(example_artifact(), &LoadLimits::default())
            .await
            .unwrap();
        let executor = prepared.create_executor().await.unwrap();
        let mut machine = MachineInstance::start(
            Arc::clone(prepared.definition()),
            executor,
            PayloadEnvelope::json(br#"{"phase":"idle"}"#.to_vec()),
            limits,
        )
        .await
        .unwrap();

        let report = machine
            .dispatch(HostInput::Event(Event::new(
                EventName::new("begin").unwrap(),
                None,
            )))
            .await
            .unwrap();
        assert_eq!(report.outcome, RunOutcome::Completed);
        assert_eq!(machine.snapshot().state, StateId::new("done").unwrap());
        assert_eq!(machine.snapshot().context.data(), br#"{"phase":"done"}"#);
    }

    #[tokio::test]
    async fn guest_action_business_failure_uses_the_failure_target() {
        let limits = RuntimeLimits::default();
        let loader = WasmMachineLoader::new(limits.invocation.clone()).unwrap();
        let prepared = loader
            .prepare(example_artifact(), &LoadLimits::default())
            .await
            .unwrap();
        let executor = prepared.create_executor().await.unwrap();
        let mut machine = MachineInstance::start(
            Arc::clone(prepared.definition()),
            executor,
            PayloadEnvelope::json(br#"{"fail":true}"#.to_vec()),
            limits,
        )
        .await
        .unwrap();

        machine
            .dispatch(HostInput::Event(Event::new(
                EventName::new("begin").unwrap(),
                None,
            )))
            .await
            .unwrap();
        assert_eq!(machine.snapshot().state, StateId::new("rejected").unwrap());
        assert!(matches!(machine.snapshot().lifecycle, Lifecycle::Failed(_)));
        assert_eq!(
            machine.snapshot().context.data(),
            br#"{"phase":"rejected"}"#
        );
    }

    async fn run_limit_event(invocation: InvocationLimits, event: &str) -> RunReport {
        let limits = RuntimeLimits {
            invocation: invocation.clone(),
            ..RuntimeLimits::default()
        };
        let loader = WasmMachineLoader::new(invocation).unwrap();
        let prepared = loader
            .prepare(example_artifact(), &LoadLimits::default())
            .await
            .unwrap();
        let executor = prepared.create_executor().await.unwrap();
        let mut machine = MachineInstance::start(
            Arc::clone(prepared.definition()),
            executor,
            PayloadEnvelope::json(br#"{"phase":"idle"}"#.to_vec()),
            limits,
        )
        .await
        .unwrap();
        machine
            .dispatch(HostInput::Event(Event::new(
                EventName::new(event).unwrap(),
                None,
            )))
            .await
            .unwrap()
    }

    fn assert_resource_limit(report: &RunReport, expected: ResourceLimitKind) {
        assert!(matches!(
            &report.outcome,
            RunOutcome::Failed(FailureRecord::Runtime(RuntimeFault {
                kind: RuntimeFaultKind::ResourceLimit(actual),
                ..
            })) if *actual == expected
        ));
    }

    #[tokio::test]
    async fn fuel_limit_stops_an_infinite_guest_action() {
        let invocation = InvocationLimits {
            cpu_budget: CpuBudget::Fuel { units: 5_000_000 },
            ..InvocationLimits::default()
        };
        let report = run_limit_event(invocation, "spin").await;
        assert_resource_limit(&report, ResourceLimitKind::Cpu);
    }

    #[tokio::test]
    async fn epoch_deadline_stops_an_infinite_guest_action() {
        let invocation = InvocationLimits {
            cpu_budget: CpuBudget::Epoch { ticks: 2 },
            ..InvocationLimits::default()
        };
        let report = run_limit_event(invocation, "spin").await;
        assert_resource_limit(&report, ResourceLimitKind::Deadline);
    }

    #[tokio::test]
    async fn memory_limit_stops_guest_linear_memory_growth() {
        let invocation = InvocationLimits {
            max_memory_bytes: 8 * 1024 * 1024,
            ..InvocationLimits::default()
        };
        let report = run_limit_event(invocation, "allocate").await;
        assert_resource_limit(&report, ResourceLimitKind::Memory);
    }

    #[tokio::test]
    async fn guest_declared_error_preserves_code_and_payload() {
        let report = run_limit_event(InvocationLimits::default(), "guest-error").await;
        assert!(matches!(
            report.outcome,
            RunOutcome::Failed(FailureRecord::Runtime(RuntimeFault {
                kind: RuntimeFaultKind::Guest,
                code: Some(ref code),
                payload: Some(ref payload),
                ..
            })) if code == "example-error" && payload.data() == br#"{"detail":true}"#
        ));
    }
}
