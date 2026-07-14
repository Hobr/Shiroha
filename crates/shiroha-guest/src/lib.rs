//! Rust guest SDK for Shiroha machine Components.

#![deny(unsafe_code)]

pub mod bindings {
    #![allow(unsafe_code)]

    wit_bindgen::generate!({
        path: "../../wit/shiroha-machine",
        world: "machine-component",
        pub_export_macro: true,
        export_macro_name: "export_shiroha_component",
    });
}

pub use bindings::exports::shiroha::machine::types;

/// High-level guest contract implemented by one self-contained machine
/// Component. The export macro bridges this trait to generated Canonical ABI
/// bindings.
pub trait MachineGuest {
    fn get_machine() -> Result<types::MachineDefinition, types::GuestError>;

    fn evaluate_guard(id: String, input: types::GuardInput) -> Result<bool, types::GuestError>;

    fn invoke_callback(
        id: String,
        input: types::HookInput,
    ) -> Result<types::HookEffects, types::GuestError>;

    fn invoke_action(
        id: String,
        input: types::HookInput,
    ) -> Result<types::ActionOutcome, types::GuestError>;
}

/// Export one [`MachineGuest`] implementation as the canonical Shiroha world.
#[macro_export]
macro_rules! export_machine {
    ($guest:ty) => {
        struct __ShirohaGuestExport;

        impl $crate::bindings::exports::shiroha::machine::definition::Guest
            for __ShirohaGuestExport
        {
            fn get_machine() -> Result<
                $crate::types::MachineDefinition,
                $crate::types::GuestError,
            > {
                <$guest as $crate::MachineGuest>::get_machine()
            }
        }

        impl $crate::bindings::exports::shiroha::machine::functions::Guest
            for __ShirohaGuestExport
        {
            fn evaluate_guard(
                id: String,
                input: $crate::types::GuardInput,
            ) -> Result<bool, $crate::types::GuestError> {
                <$guest as $crate::MachineGuest>::evaluate_guard(id, input)
            }

            fn invoke_callback(
                id: String,
                input: $crate::types::HookInput,
            ) -> Result<$crate::types::HookEffects, $crate::types::GuestError> {
                <$guest as $crate::MachineGuest>::invoke_callback(id, input)
            }

            fn invoke_action(
                id: String,
                input: $crate::types::HookInput,
            ) -> Result<$crate::types::ActionOutcome, $crate::types::GuestError> {
                <$guest as $crate::MachineGuest>::invoke_action(id, input)
            }
        }

        $crate::bindings::export_shiroha_component!(
            __ShirohaGuestExport with_types_in $crate::bindings
        );
    };
}

impl types::Payload {
    #[must_use]
    pub fn json(data: impl Into<Vec<u8>>) -> Self {
        Self {
            data: data.into(),
            content_type: JSON_CONTENT_TYPE.to_owned(),
            schema_id: None,
        }
    }
}

impl types::HookEffects {
    #[must_use]
    pub fn none() -> Self {
        Self {
            replacement_context: None,
            events: Vec::new(),
        }
    }
}

impl types::GuestError {
    #[must_use]
    pub fn unknown_function(id: &str) -> Self {
        Self {
            code: "unknown-function".to_owned(),
            message: format!("unknown guest function `{id}`"),
            payload: None,
        }
    }
}

/// MIME type required by the v0.1 JSON payload profile.
pub const JSON_CONTENT_TYPE: &str = "application/json";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_helper_uses_the_v0_1_json_profile() {
        let payload = types::Payload::json(br#"{"ok":true}"#.to_vec());
        assert_eq!(payload.content_type, JSON_CONTENT_TYPE);
        assert_eq!(payload.schema_id, None);
    }

    #[test]
    fn unknown_function_error_is_structured() {
        let error = types::GuestError::unknown_function("missing");
        assert_eq!(error.code, "unknown-function");
        assert!(error.message.contains("missing"));
    }
}
