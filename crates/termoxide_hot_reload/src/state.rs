//! The app's state as the host keeps it and the trace records it: the
//! fingerprint of its type, then the app, as a guest and a release run both
//! write it.

use serde::{Serialize, de::DeserializeOwned};

use crate::ipc::{self, IpcError};

/// `app` with `fingerprint`, the fingerprint of its type.
pub(crate) fn encode<A: Serialize>(app: &A, fingerprint: Option<u64>) -> ipc::Result<Vec<u8>> {
    let mut state = fingerprint.unwrap_or_default().to_le_bytes().to_vec();
    state.extend(postcard::to_allocvec(app).map_err(IpcError::Encode)?);
    Ok(state)
}

/// The app in `state`, or `None` when another state type saved it. A state
/// that matches the type yet doesn't decode is an error, not a reset.
pub(crate) fn decode<A: DeserializeOwned>(state: &[u8], fingerprint: Option<u64>) -> ipc::Result<Option<A>> {
    let Some((saved, app)) = state.split_first_chunk::<8>() else {
        return Ok(None);
    };
    if fingerprint.is_some_and(|fingerprint| fingerprint != u64::from_le_bytes(*saved)) {
        return Ok(None);
    }
    postcard::from_bytes(app).map(Some).map_err(IpcError::State)
}
