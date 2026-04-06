//! Shared gRPC service support helpers.

use shiroha_core::error::ShirohaError;
use tonic::Status;
use uuid::Uuid;

pub(crate) fn parse_uuid(s: &str) -> Result<Uuid, Status> {
    s.parse::<Uuid>()
        .map_err(|_| Status::invalid_argument(format!("invalid UUID: {s}")))
}

pub(crate) fn map_delete_job_error(error: ShirohaError) -> Status {
    match error {
        ShirohaError::JobNotFound(_) => Status::not_found(error.to_string()),
        ShirohaError::InvalidJobState { .. } => {
            Status::failed_precondition(error.to_string())
        }
        _ => Status::internal(error.to_string()),
    }
}
