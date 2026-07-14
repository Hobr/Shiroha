use shiroha_core::{LimitsError, RuntimeFault, ValidationErrors};
use thiserror::Error;

use crate::ConversionError;

#[derive(Debug, Error)]
pub enum WasmError {
    #[error("invalid runtime limits: {0}")]
    InvalidLimits(#[from] LimitsError),
    #[error("failed to initialize Wasmtime for `{operation}`: {diagnostic}")]
    Engine {
        operation: &'static str,
        diagnostic: String,
    },
    #[error("component artifact is empty")]
    EmptyArtifact,
    #[error("component artifact has {actual} bytes, exceeding the {maximum}-byte limit")]
    ArtifactTooLarge { actual: usize, maximum: usize },
    #[error("component declares unsupported imports: {imports:?}")]
    UnsupportedImports { imports: Vec<String> },
    #[error("component imports could not be linked: {diagnostic}")]
    Link { diagnostic: String },
    #[error("component does not implement the canonical Shiroha world: {diagnostic}")]
    IncompatibleComponent { diagnostic: String },
    #[error("guest definition function returned `{code}`: {message}")]
    GuestDefinition { code: String, message: String },
    #[error("guest definition call failed: {0}")]
    DefinitionCall(RuntimeFault),
    #[error(transparent)]
    Conversion(#[from] ConversionError),
    #[error(transparent)]
    Validation(#[from] ValidationErrors),
}

pub(crate) fn diagnostic(error: &wasmtime::Error) -> String {
    format!("{error:#}")
}
