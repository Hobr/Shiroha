use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;

/// Default maximum length for every logical identifier in the v0.1 IR.
pub const MAX_IDENTIFIER_LEN: usize = 128;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum IdentifierError {
    #[error("{kind} must not be empty")]
    Empty { kind: &'static str },
    #[error("{kind} exceeds the {max}-byte limit")]
    TooLong { kind: &'static str, max: usize },
    #[error("{kind} contains an unsupported character at byte {index}")]
    InvalidCharacter { kind: &'static str, index: usize },
}

fn validate(value: &str, kind: &'static str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::Empty { kind });
    }
    if value.len() > MAX_IDENTIFIER_LEN {
        return Err(IdentifierError::TooLong {
            kind,
            max: MAX_IDENTIFIER_LEN,
        });
    }

    if let Some((index, _)) = value.char_indices().find(|(_, character)| {
        !(character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | ':' | '/'))
    }) {
        return Err(IdentifierError::InvalidCharacter { kind, index });
    }

    Ok(())
}

macro_rules! identifier {
    ($name:ident, $kind:literal) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
                let value = value.into();
                validate(&value, $kind)?;
                Ok(Self(value))
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            #[must_use]
            pub fn into_string(self) -> String {
                self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl TryFrom<&str> for $name {
            type Error = IdentifierError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdentifierError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }
    };
}

identifier!(MachineId, "machine identifier");
identifier!(StateId, "state identifier");
identifier!(EventName, "event name");
identifier!(FunctionId, "function identifier");
identifier!(ActionKind, "action kind");
identifier!(TimeoutKey, "timeout key");

impl ActionKind {
    /// Executor kind used by the self-contained v0.1 WASM Component adapter.
    pub fn wasm_component() -> Self {
        Self("wasm-component".to_owned())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InstanceId(u64);

impl InstanceId {
    #[must_use]
    pub fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for InstanceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}
