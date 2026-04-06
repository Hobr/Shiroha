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

#[macro_export]
macro_rules! generate_flow {
    () => {
        #[allow(unused_extern_crates)]
        extern crate shiroha_sdk as wit_bindgen;
        $crate::__wit_bindgen::generate!({
            inline: r#"
                package shiroha:flow@0.1.0;

                world flow {
                  record flow-manifest {
                    id: string,
                    host-world: flow-world,
                    states: list<state-def>,
                    transitions: list<transition-def>,
                    initial-state: string,
                    actions: list<action-def>,
                  }

                  enum flow-world {
                    sandbox,
                    network,
                    storage,
                    full,
                  }

                  record state-def {
                    name: string,
                    kind: state-kind,
                    on-enter: option<string>,
                    on-exit: option<string>,
                    subprocess: option<subprocess-def>,
                  }

                  variant state-kind {
                    normal,
                    terminal,
                    fork,
                    join,
                    subprocess,
                  }

                  record subprocess-def {
                    flow-id: string,
                    completion-event: string,
                  }

                  record transition-def {
                    %from: string,
                    to: string,
                    event: string,
                    guard: option<string>,
                    action: option<string>,
                    timeout: option<timeout-def>,
                  }

                  record timeout-def {
                    duration-ms: u64,
                    timeout-event: string,
                  }

                  record action-def {
                    name: string,
                    dispatch: dispatch-mode,
                    capabilities: list<action-capability>,
                  }

                  enum action-capability {
                    network,
                    storage,
                  }

                  variant dispatch-mode {
                    local,
                    remote,
                    fan-out(fan-out-config),
                  }

                  record fan-out-config {
                    strategy: fan-out-strategy,
                    aggregator: string,
                    timeout-ms: option<u64>,
                    min-success: option<u32>,
                  }

                  variant fan-out-strategy {
                    all,
                    count(u32),
                    tagged(list<string>),
                  }

                  record action-context {
                    job-id: string,
                    state: string,
                    payload: option<list<u8>>,
                  }

                  record guard-context {
                    job-id: string,
                    from-state: string,
                    to-state: string,
                    event: string,
                    payload: option<list<u8>>,
                  }

                  enum execution-status {
                    success,
                    failed,
                    timeout,
                  }

                  record action-result {
                    status: execution-status,
                    output: option<list<u8>>,
                  }

                  record node-result {
                    node-id: string,
                    status: execution-status,
                    output: option<list<u8>>,
                  }

                  record aggregate-decision {
                    event: string,
                    context-patch: option<list<u8>>,
                  }

                  export get-manifest: func() -> flow-manifest;
                  export invoke-action: func(name: string, ctx: action-context) -> action-result;
                  export invoke-guard: func(name: string, ctx: guard-context) -> bool;
                  export aggregate: func(name: string, results: list<node-result>) -> aggregate-decision;
                }
            "#,
            world: "flow",
        });
    };
}

#[macro_export]
macro_rules! generate_network_flow {
    () => {
        #[allow(unused_extern_crates)]
        extern crate shiroha_sdk as wit_bindgen;
        $crate::__wit_bindgen::generate!({
            inline: r#"
                package shiroha:flow@0.1.0;

                interface net {
                  record header { name: string, value: string }
                  record basic-auth { username: string, password: option<string> }
                  enum http-method { get, head, post, put, delete, connect, options, trace, patch }
                  enum http-version { default, http09, http10, http11, http2, http3 }
                  variant redirect-policy { default, none, limited(u32) }
                  enum proxy-scope { all, http, https }
                  record proxy-config { scope: proxy-scope, url: string, auth: option<basic-auth> }
                  enum tls-version { tls10, tls11, tls12, tls13 }
                  record tls-config {
                    min-version: option<tls-version>,
                    max-version: option<tls-version>,
                    built-in-root-certs: option<bool>,
                    danger-accept-invalid-certs: option<bool>,
                    danger-accept-invalid-hostnames: option<bool>,
                    https-only: option<bool>,
                    root-certificates-pem: list<list<u8>>,
                  }
                  record client-config {
                    default-headers: list<header>,
                    user-agent: option<string>,
                    timeout-ms: option<u64>,
                    connect-timeout-ms: option<u64>,
                    pool-idle-timeout-ms: option<u64>,
                    pool-max-idle-per-host: option<u32>,
                    tcp-keepalive-ms: option<u64>,
                    tcp-nodelay: option<bool>,
                    referer: option<bool>,
                    gzip: option<bool>,
                    brotli: option<bool>,
                    zstd: option<bool>,
                    deflate: option<bool>,
                    cookie-store: option<bool>,
                    no-proxy: option<bool>,
                    http1-only: option<bool>,
                    http2-prior-knowledge: option<bool>,
                    redirect-policy: option<redirect-policy>,
                    proxies: list<proxy-config>,
                    tls: option<tls-config>,
                    local-address: option<string>,
                  }
                  record request-options {
                    method: http-method,
                    url: string,
                    headers: list<header>,
                    query: list<header>,
                    version: option<http-version>,
                    timeout-ms: option<u64>,
                    bearer-token: option<string>,
                    basic-auth: option<basic-auth>,
                    body: option<list<u8>>,
                    error-for-status: option<bool>,
                  }
                  record response {
                    status: u16,
                    url: string,
                    version: http-version,
                    headers: list<header>,
                    body: list<u8>,
                  }
                  enum error-kind { invalid-url, invalid-method, invalid-header, invalid-config, builder, connect, timeout, redirect, status, request, decode }
                  record error { kind: error-kind, message: string, status: option<u16>, url: option<string> }
                  send: func(client: option<client-config>, request: request-options) -> result<response, error>;
                }

                world network-flow {
                  include flow;
                  import net;
                }

                world flow {
                  record flow-manifest {
                    id: string,
                    host-world: flow-world,
                    states: list<state-def>,
                    transitions: list<transition-def>,
                    initial-state: string,
                    actions: list<action-def>,
                  }
                  enum flow-world { sandbox, network, storage, full }
                  record state-def {
                    name: string,
                    kind: state-kind,
                    on-enter: option<string>,
                    on-exit: option<string>,
                    subprocess: option<subprocess-def>,
                  }
                  variant state-kind { normal, terminal, fork, join, subprocess }
                  record subprocess-def { flow-id: string, completion-event: string }
                  record transition-def {
                    %from: string,
                    to: string,
                    event: string,
                    guard: option<string>,
                    action: option<string>,
                    timeout: option<timeout-def>,
                  }
                  record timeout-def { duration-ms: u64, timeout-event: string }
                  record action-def { name: string, dispatch: dispatch-mode, capabilities: list<action-capability> }
                  enum action-capability { network, storage }
                  variant dispatch-mode { local, remote, fan-out(fan-out-config) }
                  record fan-out-config {
                    strategy: fan-out-strategy,
                    aggregator: string,
                    timeout-ms: option<u64>,
                    min-success: option<u32>,
                  }
                  variant fan-out-strategy { all, count(u32), tagged(list<string>) }
                  record action-context { job-id: string, state: string, payload: option<list<u8>> }
                  record guard-context {
                    job-id: string,
                    from-state: string,
                    to-state: string,
                    event: string,
                    payload: option<list<u8>>,
                  }
                  enum execution-status { success, failed, timeout }
                  record action-result { status: execution-status, output: option<list<u8>> }
                  record node-result { node-id: string, status: execution-status, output: option<list<u8>> }
                  record aggregate-decision { event: string, context-patch: option<list<u8>> }
                  export get-manifest: func() -> flow-manifest;
                  export invoke-action: func(name: string, ctx: action-context) -> action-result;
                  export invoke-guard: func(name: string, ctx: guard-context) -> bool;
                  export aggregate: func(name: string, results: list<node-result>) -> aggregate-decision;
                }
            "#,
            world: "network-flow",
        });
    };
}

#[macro_export]
macro_rules! generate_storage_flow {
    () => {
        #[allow(unused_extern_crates)]
        extern crate shiroha_sdk as wit_bindgen;
        $crate::__wit_bindgen::generate!({
            inline: r#"
                package shiroha:flow@0.1.0;

                interface store {
                  get: func(namespace: string, key: string) -> option<list<u8>>;
                  put: func(namespace: string, key: string, value: list<u8>);
                  delete: func(namespace: string, key: string) -> bool;
                  list-keys: func(namespace: string, prefix: option<string>, limit: option<u32>) -> list<string>;
                }

                world storage-flow {
                  include flow;
                  import store;
                }

                world flow {
                  record flow-manifest {
                    id: string,
                    host-world: flow-world,
                    states: list<state-def>,
                    transitions: list<transition-def>,
                    initial-state: string,
                    actions: list<action-def>,
                  }
                  enum flow-world { sandbox, network, storage, full }
                  record state-def {
                    name: string,
                    kind: state-kind,
                    on-enter: option<string>,
                    on-exit: option<string>,
                    subprocess: option<subprocess-def>,
                  }
                  variant state-kind { normal, terminal, fork, join, subprocess }
                  record subprocess-def { flow-id: string, completion-event: string }
                  record transition-def {
                    %from: string,
                    to: string,
                    event: string,
                    guard: option<string>,
                    action: option<string>,
                    timeout: option<timeout-def>,
                  }
                  record timeout-def { duration-ms: u64, timeout-event: string }
                  record action-def { name: string, dispatch: dispatch-mode, capabilities: list<action-capability> }
                  enum action-capability { network, storage }
                  variant dispatch-mode { local, remote, fan-out(fan-out-config) }
                  record fan-out-config {
                    strategy: fan-out-strategy,
                    aggregator: string,
                    timeout-ms: option<u64>,
                    min-success: option<u32>,
                  }
                  variant fan-out-strategy { all, count(u32), tagged(list<string>) }
                  record action-context { job-id: string, state: string, payload: option<list<u8>> }
                  record guard-context {
                    job-id: string,
                    from-state: string,
                    to-state: string,
                    event: string,
                    payload: option<list<u8>>,
                  }
                  enum execution-status { success, failed, timeout }
                  record action-result { status: execution-status, output: option<list<u8>> }
                  record node-result { node-id: string, status: execution-status, output: option<list<u8>> }
                  record aggregate-decision { event: string, context-patch: option<list<u8>> }
                  export get-manifest: func() -> flow-manifest;
                  export invoke-action: func(name: string, ctx: action-context) -> action-result;
                  export invoke-guard: func(name: string, ctx: guard-context) -> bool;
                  export aggregate: func(name: string, results: list<node-result>) -> aggregate-decision;
                }
            "#,
            world: "storage-flow",
        });
    };
}

#[macro_export]
macro_rules! generate_full_flow {
    () => {
        #[allow(unused_extern_crates)]
        extern crate shiroha_sdk as wit_bindgen;
        $crate::__wit_bindgen::generate!({
            inline: r#"
                package shiroha:flow@0.1.0;

                interface net {
                  record header { name: string, value: string }
                  record basic-auth { username: string, password: option<string> }
                  enum http-method { get, head, post, put, delete, connect, options, trace, patch }
                  enum http-version { default, http09, http10, http11, http2, http3 }
                  variant redirect-policy { default, none, limited(u32) }
                  enum proxy-scope { all, http, https }
                  record proxy-config { scope: proxy-scope, url: string, auth: option<basic-auth> }
                  enum tls-version { tls10, tls11, tls12, tls13 }
                  record tls-config {
                    min-version: option<tls-version>,
                    max-version: option<tls-version>,
                    built-in-root-certs: option<bool>,
                    danger-accept-invalid-certs: option<bool>,
                    danger-accept-invalid-hostnames: option<bool>,
                    https-only: option<bool>,
                    root-certificates-pem: list<list<u8>>,
                  }
                  record client-config {
                    default-headers: list<header>,
                    user-agent: option<string>,
                    timeout-ms: option<u64>,
                    connect-timeout-ms: option<u64>,
                    pool-idle-timeout-ms: option<u64>,
                    pool-max-idle-per-host: option<u32>,
                    tcp-keepalive-ms: option<u64>,
                    tcp-nodelay: option<bool>,
                    referer: option<bool>,
                    gzip: option<bool>,
                    brotli: option<bool>,
                    zstd: option<bool>,
                    deflate: option<bool>,
                    cookie-store: option<bool>,
                    no-proxy: option<bool>,
                    http1-only: option<bool>,
                    http2-prior-knowledge: option<bool>,
                    redirect-policy: option<redirect-policy>,
                    proxies: list<proxy-config>,
                    tls: option<tls-config>,
                    local-address: option<string>,
                  }
                  record request-options {
                    method: http-method,
                    url: string,
                    headers: list<header>,
                    query: list<header>,
                    version: option<http-version>,
                    timeout-ms: option<u64>,
                    bearer-token: option<string>,
                    basic-auth: option<basic-auth>,
                    body: option<list<u8>>,
                    error-for-status: option<bool>,
                  }
                  record response {
                    status: u16,
                    url: string,
                    version: http-version,
                    headers: list<header>,
                    body: list<u8>,
                  }
                  enum error-kind { invalid-url, invalid-method, invalid-header, invalid-config, builder, connect, timeout, redirect, status, request, decode }
                  record error { kind: error-kind, message: string, status: option<u16>, url: option<string> }
                  send: func(client: option<client-config>, request: request-options) -> result<response, error>;
                }

                interface store {
                  get: func(namespace: string, key: string) -> option<list<u8>>;
                  put: func(namespace: string, key: string, value: list<u8>);
                  delete: func(namespace: string, key: string) -> bool;
                  list-keys: func(namespace: string, prefix: option<string>, limit: option<u32>) -> list<string>;
                }

                world full-flow {
                  include flow;
                  import net;
                  import store;
                }

                world flow {
                  record flow-manifest {
                    id: string,
                    host-world: flow-world,
                    states: list<state-def>,
                    transitions: list<transition-def>,
                    initial-state: string,
                    actions: list<action-def>,
                  }
                  enum flow-world { sandbox, network, storage, full }
                  record state-def {
                    name: string,
                    kind: state-kind,
                    on-enter: option<string>,
                    on-exit: option<string>,
                    subprocess: option<subprocess-def>,
                  }
                  variant state-kind { normal, terminal, fork, join, subprocess }
                  record subprocess-def { flow-id: string, completion-event: string }
                  record transition-def {
                    %from: string,
                    to: string,
                    event: string,
                    guard: option<string>,
                    action: option<string>,
                    timeout: option<timeout-def>,
                  }
                  record timeout-def { duration-ms: u64, timeout-event: string }
                  record action-def { name: string, dispatch: dispatch-mode, capabilities: list<action-capability> }
                  enum action-capability { network, storage }
                  variant dispatch-mode { local, remote, fan-out(fan-out-config) }
                  record fan-out-config {
                    strategy: fan-out-strategy,
                    aggregator: string,
                    timeout-ms: option<u64>,
                    min-success: option<u32>,
                  }
                  variant fan-out-strategy { all, count(u32), tagged(list<string>) }
                  record action-context { job-id: string, state: string, payload: option<list<u8>> }
                  record guard-context {
                    job-id: string,
                    from-state: string,
                    to-state: string,
                    event: string,
                    payload: option<list<u8>>,
                  }
                  enum execution-status { success, failed, timeout }
                  record action-result { status: execution-status, output: option<list<u8>> }
                  record node-result { node-id: string, status: execution-status, output: option<list<u8>> }
                  record aggregate-decision { event: string, context-patch: option<list<u8>> }
                  export get-manifest: func() -> flow-manifest;
                  export invoke-action: func(name: string, ctx: action-context) -> action-result;
                  export invoke-guard: func(name: string, ctx: guard-context) -> bool;
                  export aggregate: func(name: string, results: list<node-result>) -> aggregate-decision;
                }
            "#,
            world: "full-flow",
        });
    };
}

#[macro_export]
macro_rules! action_ok {
    ($output:expr) => {
        ActionResult {
            status: ExecutionStatus::Success,
            output: $output,
        }
    };
}

#[macro_export]
macro_rules! action_fail {
    ($output:expr) => {
        ActionResult {
            status: ExecutionStatus::Failed,
            output: $output,
        }
    };
}

#[macro_export]
macro_rules! aggregate_event {
    ($event:expr, $context_patch:expr) => {
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
        $(, caps: [$($capability:ident),* $(,)?])?
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
            $(, caps: [$($($capability),*)?])?
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
