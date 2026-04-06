pub use wit_bindgen as __wit_bindgen;
pub use wit_bindgen::*;

pub mod prelude {
    pub use crate::{
        action_fail, action_ok, aggregate_event, capabilities, fanout_action, flow_action,
        flow_manifest, flow_state, flow_subprocess, flow_timeout, flow_transition, local_action,
        remote_action,
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __sdk_option_string {
    () => {
        None
    };
    ($value:expr) => {
        Some($value.to_string())
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __sdk_option_expr {
    () => {
        None
    };
    ($value:expr) => {
        Some($value)
    };
}

include!(concat!(env!("OUT_DIR"), "/generated_wit_macros.rs"));

#[macro_export]
macro_rules! action_ok {
    ($output:expr $(,)?) => {
        ActionResult {
            status: ExecutionStatus::Success,
            output: $output,
        }
    };
}

#[macro_export]
macro_rules! action_fail {
    ($output:expr $(,)?) => {
        ActionResult {
            status: ExecutionStatus::Failed,
            output: $output,
        }
    };
}

#[macro_export]
macro_rules! aggregate_event {
    ($event:expr, $context_patch:expr $(,)?) => {
        AggregateDecision {
            event: $event,
            context_patch: $context_patch,
        }
    };
}

#[macro_export]
macro_rules! capabilities {
    () => {
        Vec::new()
    };
    [$($capability:ident),+ $(,)?] => {
        vec![$(ActionCapability::$capability),+]
    };
}

#[macro_export]
macro_rules! flow_timeout {
    ($duration_ms:expr, $timeout_event:expr $(,)?) => {
        TimeoutDef {
            duration_ms: $duration_ms,
            timeout_event: $timeout_event.to_string(),
        }
    };
}

#[macro_export]
macro_rules! flow_subprocess {
    ($flow_id:expr, $completion_event:expr $(,)?) => {
        SubprocessDef {
            flow_id: $flow_id.to_string(),
            completion_event: $completion_event.to_string(),
        }
    };
}

#[macro_export]
macro_rules! flow_state {
    ($name:expr, $kind:ident $(, on_enter: $on_enter:expr)? $(, on_exit: $on_exit:expr)? $(, subprocess: $subprocess:expr)? $(,)?) => {
        StateDef {
            name: $name.to_string(),
            kind: StateKind::$kind,
            on_enter: $crate::__sdk_option_string!($($on_enter)?),
            on_exit: $crate::__sdk_option_string!($($on_exit)?),
            subprocess: $crate::__sdk_option_expr!($($subprocess)?),
        }
    };
}

#[macro_export]
macro_rules! flow_transition {
    ($from:expr, $event:expr, $to:expr $(, guard: $guard:expr)? $(, action: $action:expr)? $(, timeout: $timeout:expr)? $(,)?) => {
        TransitionDef {
            from: $from.to_string(),
            to: $to.to_string(),
            event: $event.to_string(),
            guard: $crate::__sdk_option_string!($($guard)?),
            action: $crate::__sdk_option_string!($($action)?),
            timeout: $crate::__sdk_option_expr!($($timeout)?),
        }
    };
}

#[macro_export]
macro_rules! flow_action {
    ($name:expr, $dispatch:expr $(, caps: [$($capability:ident),* $(,)?])? $(,)?) => {
        ActionDef {
            name: $name.to_string(),
            dispatch: $dispatch,
            capabilities: $crate::capabilities![$($($capability),*)?],
        }
    };
}

#[macro_export]
macro_rules! local_action {
    ($name:expr $(,)?) => {
        $crate::flow_action!($name, DispatchMode::Local)
    };
    ($name:expr, caps: [$($capability:ident),* $(,)?] $(,)?) => {
        $crate::flow_action!($name, DispatchMode::Local, caps: [$($capability),*])
    };
}

#[macro_export]
macro_rules! remote_action {
    ($name:expr $(,)?) => {
        $crate::flow_action!($name, DispatchMode::Remote)
    };
    ($name:expr, caps: [$($capability:ident),* $(,)?] $(,)?) => {
        $crate::flow_action!($name, DispatchMode::Remote, caps: [$($capability),*])
    };
}

#[macro_export]
macro_rules! fanout_action {
    (
        $name:expr,
        strategy: $strategy:expr,
        aggregator: $aggregator:expr
        $(, timeout_ms: $timeout_ms:expr)?
        $(, min_success: $min_success:expr)?
        $(,)?
    ) => {
        $crate::flow_action!(
            $name,
            DispatchMode::FanOut(FanOutConfig {
                strategy: $strategy,
                aggregator: $aggregator.to_string(),
                timeout_ms: $crate::__sdk_option_expr!($($timeout_ms)?),
                min_success: $crate::__sdk_option_expr!($($min_success)?),
            })
        )
    };
    (
        $name:expr,
        strategy: $strategy:expr,
        aggregator: $aggregator:expr
        $(, timeout_ms: $timeout_ms:expr)?
        $(, min_success: $min_success:expr)?
        , caps: [$($capability:ident),* $(,)?]
        $(,)?
    ) => {
        $crate::flow_action!(
            $name,
            DispatchMode::FanOut(FanOutConfig {
                strategy: $strategy,
                aggregator: $aggregator.to_string(),
                timeout_ms: $crate::__sdk_option_expr!($($timeout_ms)?),
                min_success: $crate::__sdk_option_expr!($($min_success)?),
            }),
            caps: [$($capability),*]
        )
    };
}

#[macro_export]
macro_rules! flow_manifest {
    (
        id: $id:expr,
        world: $world:ident,
        states: $states:expr,
        transitions: $transitions:expr,
        initial_state: $initial_state:expr,
        actions: $actions:expr
        $(,)?
    ) => {
        FlowManifest {
            id: $id.to_string(),
            host_world: FlowWorld::$world,
            states: $states,
            transitions: $transitions,
            initial_state: $initial_state.to_string(),
            actions: $actions,
        }
    };
}
