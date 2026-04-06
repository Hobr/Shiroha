#![allow(dead_code)]

use std::net::IpAddr;
use std::str::FromStr;
use std::time::Duration;

use reqwest::blocking::{ClientBuilder, Response};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::redirect::Policy;
use reqwest::{Certificate, Method, Proxy, Version};
use wasmtime::component::{ComponentType, Lift, Lower};

use crate::error::WasmError;

use super::ComponentStoreState;

#[derive(Debug, Clone, PartialEq, Eq, ComponentType, Lift, Lower)]
#[component(record)]
struct NetworkHeader {
    name: String,
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, ComponentType, Lift, Lower)]
#[component(record)]
struct NetworkBasicAuth {
    username: String,
    password: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ComponentType, Lift, Lower)]
#[component(enum)]
#[repr(u8)]
enum NetworkHttpMethod {
    #[component(name = "get")]
    Get,
    #[component(name = "head")]
    Head,
    #[component(name = "post")]
    Post,
    #[component(name = "put")]
    Put,
    #[component(name = "delete")]
    Delete,
    #[component(name = "connect")]
    Connect,
    #[component(name = "options")]
    Options,
    #[component(name = "trace")]
    Trace,
    #[component(name = "patch")]
    Patch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ComponentType, Lift, Lower)]
#[component(enum)]
#[repr(u8)]
enum NetworkHttpVersion {
    #[component(name = "default")]
    Default,
    #[component(name = "http09")]
    Http09,
    #[component(name = "http10")]
    Http10,
    #[component(name = "http11")]
    Http11,
    #[component(name = "http2")]
    Http2,
    #[component(name = "http3")]
    Http3,
}

#[derive(Debug, Clone, PartialEq, Eq, ComponentType, Lift, Lower)]
#[component(variant)]
enum NetworkRedirectPolicy {
    #[component(name = "default")]
    Default,
    #[component(name = "none")]
    None,
    #[component(name = "limited")]
    Limited(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ComponentType, Lift, Lower)]
#[component(enum)]
#[repr(u8)]
enum NetworkProxyScope {
    #[component(name = "all")]
    All,
    #[component(name = "http")]
    Http,
    #[component(name = "https")]
    Https,
}

#[derive(Debug, Clone, PartialEq, Eq, ComponentType, Lift, Lower)]
#[component(record)]
struct NetworkProxyConfig {
    scope: NetworkProxyScope,
    url: String,
    auth: Option<NetworkBasicAuth>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ComponentType, Lift, Lower)]
#[component(enum)]
#[repr(u8)]
enum NetworkTlsVersion {
    #[component(name = "tls10")]
    Tls10,
    #[component(name = "tls11")]
    Tls11,
    #[component(name = "tls12")]
    Tls12,
    #[component(name = "tls13")]
    Tls13,
}

#[derive(Debug, Clone, PartialEq, Eq, ComponentType, Lift, Lower)]
#[component(record)]
struct NetworkTlsConfig {
    #[component(name = "min-version")]
    min_version: Option<NetworkTlsVersion>,
    #[component(name = "max-version")]
    max_version: Option<NetworkTlsVersion>,
    #[component(name = "built-in-root-certs")]
    built_in_root_certs: Option<bool>,
    #[component(name = "danger-accept-invalid-certs")]
    danger_accept_invalid_certs: Option<bool>,
    #[component(name = "danger-accept-invalid-hostnames")]
    danger_accept_invalid_hostnames: Option<bool>,
    #[component(name = "https-only")]
    https_only: Option<bool>,
    #[component(name = "root-certificates-pem")]
    root_certificates_pem: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, ComponentType, Lift, Lower)]
#[component(record)]
struct NetworkClientConfig {
    #[component(name = "default-headers")]
    default_headers: Vec<NetworkHeader>,
    #[component(name = "user-agent")]
    user_agent: Option<String>,
    #[component(name = "timeout-ms")]
    timeout_ms: Option<u64>,
    #[component(name = "connect-timeout-ms")]
    connect_timeout_ms: Option<u64>,
    #[component(name = "pool-idle-timeout-ms")]
    pool_idle_timeout_ms: Option<u64>,
    #[component(name = "pool-max-idle-per-host")]
    pool_max_idle_per_host: Option<u32>,
    #[component(name = "tcp-keepalive-ms")]
    tcp_keepalive_ms: Option<u64>,
    #[component(name = "tcp-nodelay")]
    tcp_nodelay: Option<bool>,
    referer: Option<bool>,
    gzip: Option<bool>,
    brotli: Option<bool>,
    zstd: Option<bool>,
    deflate: Option<bool>,
    #[component(name = "cookie-store")]
    cookie_store: Option<bool>,
    #[component(name = "no-proxy")]
    no_proxy: Option<bool>,
    #[component(name = "http1-only")]
    http1_only: Option<bool>,
    #[component(name = "http2-prior-knowledge")]
    http2_prior_knowledge: Option<bool>,
    #[component(name = "redirect-policy")]
    redirect_policy: Option<NetworkRedirectPolicy>,
    proxies: Vec<NetworkProxyConfig>,
    tls: Option<NetworkTlsConfig>,
    #[component(name = "local-address")]
    local_address: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, ComponentType, Lift, Lower)]
#[component(record)]
struct NetworkRequestOptions {
    method: NetworkHttpMethod,
    url: String,
    headers: Vec<NetworkHeader>,
    query: Vec<NetworkHeader>,
    version: Option<NetworkHttpVersion>,
    #[component(name = "timeout-ms")]
    timeout_ms: Option<u64>,
    #[component(name = "bearer-token")]
    bearer_token: Option<String>,
    #[component(name = "basic-auth")]
    basic_auth: Option<NetworkBasicAuth>,
    body: Option<Vec<u8>>,
    #[component(name = "error-for-status")]
    error_for_status: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, ComponentType, Lift, Lower)]
#[component(record)]
struct NetworkResponse {
    status: u16,
    url: String,
    version: NetworkHttpVersion,
    headers: Vec<NetworkHeader>,
    body: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ComponentType, Lift, Lower)]
#[component(enum)]
#[repr(u8)]
enum NetworkErrorKind {
    #[component(name = "invalid-url")]
    InvalidUrl,
    #[component(name = "invalid-method")]
    InvalidMethod,
    #[component(name = "invalid-header")]
    InvalidHeader,
    #[component(name = "invalid-config")]
    InvalidConfig,
    #[component(name = "builder")]
    Builder,
    #[component(name = "connect")]
    Connect,
    #[component(name = "timeout")]
    Timeout,
    #[component(name = "redirect")]
    Redirect,
    #[component(name = "status")]
    Status,
    #[component(name = "request")]
    Request,
    #[component(name = "decode")]
    Decode,
}

#[derive(Debug, Clone, PartialEq, Eq, ComponentType, Lift, Lower)]
#[component(record)]
struct NetworkError {
    kind: NetworkErrorKind,
    message: String,
    status: Option<u16>,
    url: Option<String>,
}

pub(super) fn add_to_linker(
    linker: &mut wasmtime::component::Linker<ComponentStoreState>,
) -> Result<(), WasmError> {
    let mut inst = linker
        .instance("shiroha:flow/net@0.1.0")
        .map_err(|e| WasmError::Instantiation(e.to_string()))?;
    inst.func_wrap(
        "send",
        |caller: wasmtime::StoreContextMut<'_, ComponentStoreState>,
         (client, request): (Option<NetworkClientConfig>, NetworkRequestOptions)| {
            if !caller.data().allow_network {
                return Err(wasmtime::Error::msg(
                    "network capability is not allowed in the current invocation",
                ));
            }
            Ok((send(client, request),))
        },
    )
    .map_err(|e| WasmError::Instantiation(e.to_string()))?;
    Ok(())
}

fn send(
    client: Option<NetworkClientConfig>,
    request: NetworkRequestOptions,
) -> Result<NetworkResponse, NetworkError> {
    let client = build_client(client)?;
    let method = map_method(request.method)?;
    let mut builder = client.request(method, &request.url);

    builder = apply_headers(builder, request.headers)?;
    if !request.query.is_empty() {
        let query = request
            .query
            .iter()
            .map(|entry| (entry.name.as_str(), entry.value.as_str()))
            .collect::<Vec<_>>();
        builder = builder.query(&query);
    }
    if let Some(version) = request.version {
        builder = builder.version(map_version(version));
    }
    if let Some(timeout_ms) = request.timeout_ms {
        builder = builder.timeout(Duration::from_millis(timeout_ms));
    }
    if let Some(token) = request.bearer_token {
        builder = builder.bearer_auth(token);
    }
    if let Some(auth) = request.basic_auth {
        builder = builder.basic_auth(auth.username, auth.password);
    }
    if let Some(body) = request.body {
        builder = builder.body(body);
    }

    let response = builder.send().map_err(network_error_from_reqwest)?;
    let response = if request.error_for_status.unwrap_or(false) {
        response
            .error_for_status()
            .map_err(network_error_from_reqwest)?
    } else {
        response
    };
    response_to_component(response).map_err(network_error_from_reqwest)
}

fn build_client(
    config: Option<NetworkClientConfig>,
) -> Result<reqwest::blocking::Client, NetworkError> {
    let mut builder = ClientBuilder::new();

    if let Some(config) = config {
        builder = builder.default_headers(header_map_from_entries(config.default_headers)?);
        if let Some(user_agent) = config.user_agent {
            builder = builder.user_agent(user_agent);
        }
        if let Some(timeout_ms) = config.timeout_ms {
            builder = builder.timeout(Duration::from_millis(timeout_ms));
        }
        if let Some(timeout_ms) = config.connect_timeout_ms {
            builder = builder.connect_timeout(Duration::from_millis(timeout_ms));
        }
        if let Some(timeout_ms) = config.pool_idle_timeout_ms {
            builder = builder.pool_idle_timeout(Duration::from_millis(timeout_ms));
        }
        if let Some(max_idle) = config.pool_max_idle_per_host {
            builder = builder.pool_max_idle_per_host(max_idle as usize);
        }
        if let Some(keepalive_ms) = config.tcp_keepalive_ms {
            builder = builder.tcp_keepalive(Duration::from_millis(keepalive_ms));
        }
        if let Some(enabled) = config.tcp_nodelay {
            builder = builder.tcp_nodelay(enabled);
        }
        if let Some(enabled) = config.referer {
            builder = builder.referer(enabled);
        }
        if let Some(enabled) = config.gzip {
            builder = builder.gzip(enabled);
        }
        if let Some(enabled) = config.brotli {
            builder = builder.brotli(enabled);
        }
        if let Some(enabled) = config.zstd {
            builder = builder.zstd(enabled);
        }
        if let Some(enabled) = config.deflate {
            builder = builder.deflate(enabled);
        }
        if let Some(enabled) = config.cookie_store {
            builder = builder.cookie_store(enabled);
        }
        if config.no_proxy.unwrap_or(false) {
            builder = builder.no_proxy();
        }
        if config.http1_only.unwrap_or(false) {
            builder = builder.http1_only();
        }
        if config.http2_prior_knowledge.unwrap_or(false) {
            builder = builder.http2_prior_knowledge();
        }
        if let Some(policy) = config.redirect_policy {
            builder = builder.redirect(match policy {
                NetworkRedirectPolicy::Default => Policy::default(),
                NetworkRedirectPolicy::None => Policy::none(),
                NetworkRedirectPolicy::Limited(limit) => Policy::limited(limit as usize),
            });
        }
        for proxy in config.proxies {
            builder = builder.proxy(build_proxy(proxy)?);
        }
        if let Some(tls) = config.tls {
            if let Some(enabled) = tls.built_in_root_certs {
                builder = builder.tls_built_in_root_certs(enabled);
            }
            if let Some(enabled) = tls.danger_accept_invalid_certs {
                builder = builder.danger_accept_invalid_certs(enabled);
            }
            if let Some(enabled) = tls.danger_accept_invalid_hostnames {
                builder = builder.danger_accept_invalid_hostnames(enabled);
            }
            if let Some(enabled) = tls.https_only {
                builder = builder.https_only(enabled);
            }
            if let Some(version) = tls.min_version {
                builder = builder.min_tls_version(map_tls_version(version));
            }
            if let Some(version) = tls.max_version {
                builder = builder.max_tls_version(map_tls_version(version));
            }
            for pem_bundle in tls.root_certificates_pem {
                let certs =
                    Certificate::from_pem_bundle(&pem_bundle).map_err(|error| NetworkError {
                        kind: NetworkErrorKind::InvalidConfig,
                        message: format!("failed to parse root certificates: {error}"),
                        status: None,
                        url: None,
                    })?;
                for cert in certs {
                    builder = builder.add_root_certificate(cert);
                }
            }
        }
        if let Some(addr) = config.local_address {
            let ip = IpAddr::from_str(&addr).map_err(|error| NetworkError {
                kind: NetworkErrorKind::InvalidConfig,
                message: format!("invalid local address `{addr}`: {error}"),
                status: None,
                url: None,
            })?;
            builder = builder.local_address(ip);
        }
    }

    builder.build().map_err(network_error_from_reqwest)
}

fn build_proxy(config: NetworkProxyConfig) -> Result<Proxy, NetworkError> {
    let mut proxy = match config.scope {
        NetworkProxyScope::All => Proxy::all(&config.url),
        NetworkProxyScope::Http => Proxy::http(&config.url),
        NetworkProxyScope::Https => Proxy::https(&config.url),
    }
    .map_err(network_error_from_reqwest)?;

    if let Some(auth) = config.auth {
        proxy = proxy.basic_auth(&auth.username, auth.password.as_deref().unwrap_or(""));
    }

    Ok(proxy)
}

fn apply_headers(
    mut builder: reqwest::blocking::RequestBuilder,
    headers: Vec<NetworkHeader>,
) -> Result<reqwest::blocking::RequestBuilder, NetworkError> {
    for header in headers {
        let name =
            HeaderName::from_bytes(header.name.as_bytes()).map_err(|error| NetworkError {
                kind: NetworkErrorKind::InvalidHeader,
                message: format!("invalid header name `{}`: {error}", header.name),
                status: None,
                url: None,
            })?;
        let value = HeaderValue::from_str(&header.value).map_err(|error| NetworkError {
            kind: NetworkErrorKind::InvalidHeader,
            message: format!("invalid header value for `{}`: {error}", header.name),
            status: None,
            url: None,
        })?;
        builder = builder.header(name, value);
    }
    Ok(builder)
}

fn header_map_from_entries(headers: Vec<NetworkHeader>) -> Result<HeaderMap, NetworkError> {
    let mut map = HeaderMap::new();
    for header in headers {
        let name =
            HeaderName::from_bytes(header.name.as_bytes()).map_err(|error| NetworkError {
                kind: NetworkErrorKind::InvalidHeader,
                message: format!("invalid header name `{}`: {error}", header.name),
                status: None,
                url: None,
            })?;
        let value = HeaderValue::from_str(&header.value).map_err(|error| NetworkError {
            kind: NetworkErrorKind::InvalidHeader,
            message: format!("invalid header value for `{}`: {error}", header.name),
            status: None,
            url: None,
        })?;
        map.append(name, value);
    }
    Ok(map)
}

fn map_method(method: NetworkHttpMethod) -> Result<Method, NetworkError> {
    Ok(match method {
        NetworkHttpMethod::Get => Method::GET,
        NetworkHttpMethod::Head => Method::HEAD,
        NetworkHttpMethod::Post => Method::POST,
        NetworkHttpMethod::Put => Method::PUT,
        NetworkHttpMethod::Delete => Method::DELETE,
        NetworkHttpMethod::Connect => Method::CONNECT,
        NetworkHttpMethod::Options => Method::OPTIONS,
        NetworkHttpMethod::Trace => Method::TRACE,
        NetworkHttpMethod::Patch => Method::PATCH,
    })
}

fn map_version(version: NetworkHttpVersion) -> Version {
    match version {
        NetworkHttpVersion::Default => Version::default(),
        NetworkHttpVersion::Http09 => Version::HTTP_09,
        NetworkHttpVersion::Http10 => Version::HTTP_10,
        NetworkHttpVersion::Http11 => Version::HTTP_11,
        NetworkHttpVersion::Http2 => Version::HTTP_2,
        NetworkHttpVersion::Http3 => Version::HTTP_3,
    }
}

fn map_tls_version(version: NetworkTlsVersion) -> reqwest::tls::Version {
    match version {
        NetworkTlsVersion::Tls10 => reqwest::tls::Version::TLS_1_0,
        NetworkTlsVersion::Tls11 => reqwest::tls::Version::TLS_1_1,
        NetworkTlsVersion::Tls12 => reqwest::tls::Version::TLS_1_2,
        NetworkTlsVersion::Tls13 => reqwest::tls::Version::TLS_1_3,
    }
}

fn map_response_version(version: Version) -> NetworkHttpVersion {
    match version {
        Version::HTTP_09 => NetworkHttpVersion::Http09,
        Version::HTTP_10 => NetworkHttpVersion::Http10,
        Version::HTTP_11 => NetworkHttpVersion::Http11,
        Version::HTTP_2 => NetworkHttpVersion::Http2,
        Version::HTTP_3 => NetworkHttpVersion::Http3,
        _ => NetworkHttpVersion::Default,
    }
}

fn response_to_component(response: Response) -> reqwest::Result<NetworkResponse> {
    let status = response.status().as_u16();
    let url = response.url().to_string();
    let version = map_response_version(response.version());
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| NetworkHeader {
            name: name.as_str().to_string(),
            value: value.to_str().unwrap_or_default().to_string(),
        })
        .collect::<Vec<_>>();
    let body = response.bytes()?.to_vec();

    Ok(NetworkResponse {
        status,
        url,
        version,
        headers,
        body,
    })
}

fn network_error_from_reqwest(error: reqwest::Error) -> NetworkError {
    let kind = if error.is_builder() {
        NetworkErrorKind::Builder
    } else if error.is_connect() {
        NetworkErrorKind::Connect
    } else if error.is_timeout() {
        NetworkErrorKind::Timeout
    } else if error.is_redirect() {
        NetworkErrorKind::Redirect
    } else if error.is_status() {
        NetworkErrorKind::Status
    } else if error.is_decode() {
        NetworkErrorKind::Decode
    } else {
        NetworkErrorKind::Request
    };

    NetworkError {
        kind,
        message: error.to_string(),
        status: error.status().map(|status| status.as_u16()),
        url: error.url().map(ToString::to_string),
    }
}
