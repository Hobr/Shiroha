use std::sync::Arc;

use async_trait::async_trait;
use shiroha_core::{
    ActionKind, ActionOutcome, FunctionExecutor, FunctionExecutorFactory, FunctionRef, GuardInput,
    HookEffects, HookInput, InvocationLimits, RuntimeFault, RuntimeFaultKind,
};
use tokio::time::timeout;
use tracing::{debug, instrument};
use wasmtime::Store;

use crate::bindings::{MachineComponent, MachineComponentPre};
use crate::convert::{
    action_from_wit, effects_from_wit, guard_input_to_wit, guest_error_to_fault, hook_input_to_wit,
};
use crate::runtime::{
    RuntimeInner, StoreState, classify_wasmtime_error, create_store, deadline_fault, reset_budget,
};

pub struct WasmExecutorFactory {
    runtime: Arc<RuntimeInner>,
    pre: MachineComponentPre<StoreState>,
    limits: InvocationLimits,
}

impl WasmExecutorFactory {
    pub(crate) fn new(
        runtime: Arc<RuntimeInner>,
        pre: MachineComponentPre<StoreState>,
        limits: InvocationLimits,
    ) -> Self {
        Self {
            runtime,
            pre,
            limits,
        }
    }
}

#[async_trait]
impl FunctionExecutorFactory for WasmExecutorFactory {
    async fn create(&self) -> Result<Box<dyn FunctionExecutor>, RuntimeFault> {
        let mut store = create_store(&self.runtime, &self.limits)?;
        let component = self
            .pre
            .instantiate_async(&mut store)
            .await
            .map_err(classify_wasmtime_error)?;
        Ok(Box::new(WasmExecutor {
            store,
            component,
            poisoned: false,
        }))
    }
}

struct WasmExecutor {
    store: Store<StoreState>,
    component: MachineComponent,
    poisoned: bool,
}

impl WasmExecutor {
    fn prepare_call(
        &mut self,
        function: &FunctionRef,
        limits: &InvocationLimits,
    ) -> Result<(), RuntimeFault> {
        if self.poisoned {
            return Err(RuntimeFault::new(
                RuntimeFaultKind::Host,
                "guest executor is poisoned after a previous runtime fault",
            ));
        }
        if function.kind != ActionKind::wasm_component() {
            return Err(RuntimeFault::new(
                RuntimeFaultKind::Host,
                format!("unsupported executor kind `{}`", function.kind),
            ));
        }
        reset_budget(&mut self.store, limits)
    }

    fn observe_fault(&mut self, fault: &RuntimeFault) {
        if matches!(
            fault.kind,
            RuntimeFaultKind::Trap | RuntimeFaultKind::Engine | RuntimeFaultKind::ResourceLimit(_)
        ) {
            self.poisoned = true;
        }
    }

    fn conversion_fault(error: crate::ConversionError) -> RuntimeFault {
        RuntimeFault::new(RuntimeFaultKind::Guest, error.to_string())
            .with_guest_details("invalid-guest-output", None)
    }
}

#[async_trait]
impl FunctionExecutor for WasmExecutor {
    #[instrument(name = "shiroha.guest.guard", skip_all, fields(function = %function.locator))]
    async fn evaluate_guard(
        &mut self,
        function: &FunctionRef,
        input: GuardInput,
        limits: &InvocationLimits,
    ) -> Result<bool, RuntimeFault> {
        self.prepare_call(function, limits)?;
        let input = guard_input_to_wit(input);
        let call = self
            .component
            .shiroha_machine_functions()
            .call_evaluate_guard(&mut self.store, function.locator.as_str(), &input);
        let result = match timeout(limits.wall_time, call).await {
            Ok(Ok(result)) => result.map_err(guest_error_to_fault),
            Ok(Err(error)) => Err(classify_wasmtime_error(error)),
            Err(_) => Err(deadline_fault()),
        };
        if let Err(fault) = &result {
            self.observe_fault(fault);
        }
        result
    }

    #[instrument(name = "shiroha.guest.callback", skip_all, fields(function = %function.locator))]
    async fn invoke_callback(
        &mut self,
        function: &FunctionRef,
        input: HookInput,
        limits: &InvocationLimits,
    ) -> Result<HookEffects, RuntimeFault> {
        self.prepare_call(function, limits)?;
        let input = hook_input_to_wit(input);
        let call = self
            .component
            .shiroha_machine_functions()
            .call_invoke_callback(&mut self.store, function.locator.as_str(), &input);
        let lifted = match timeout(limits.wall_time, call).await {
            Ok(Ok(result)) => result.map_err(guest_error_to_fault),
            Ok(Err(error)) => Err(classify_wasmtime_error(error)),
            Err(_) => Err(deadline_fault()),
        };
        let result =
            lifted.and_then(|effects| effects_from_wit(effects).map_err(Self::conversion_fault));
        if let Err(fault) = &result {
            self.observe_fault(fault);
        }
        result
    }

    #[instrument(name = "shiroha.guest.action", skip_all, fields(function = %function.locator))]
    async fn invoke_action(
        &mut self,
        function: &FunctionRef,
        input: HookInput,
        limits: &InvocationLimits,
    ) -> Result<ActionOutcome, RuntimeFault> {
        self.prepare_call(function, limits)?;
        let input = hook_input_to_wit(input);
        let call = self
            .component
            .shiroha_machine_functions()
            .call_invoke_action(&mut self.store, function.locator.as_str(), &input);
        let lifted = match timeout(limits.wall_time, call).await {
            Ok(Ok(result)) => result.map_err(guest_error_to_fault),
            Ok(Err(error)) => Err(classify_wasmtime_error(error)),
            Err(_) => Err(deadline_fault()),
        };
        let result =
            lifted.and_then(|outcome| action_from_wit(outcome).map_err(Self::conversion_fault));
        if let Err(fault) = &result {
            self.observe_fault(fault);
        }
        debug!(poisoned = self.poisoned, "guest action completed");
        result
    }
}
