//! Shared gRPC service support helpers.

use tonic::Status;
use uuid::Uuid;

pub(crate) fn parse_uuid(s: &str) -> Result<Uuid, Status> {
    s.parse::<Uuid>()
        .map_err(|_| Status::invalid_argument(format!("invalid UUID: {s}")))
}
