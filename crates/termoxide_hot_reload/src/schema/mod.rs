//! Fingerprints of what crosses as bytes, so a change in their encoding is
//! caught instead of decoding wrongly: `postcard` writes no field names and
//! numbers enum variants, so a reordered field or an inserted variant still
//! decodes, as the wrong value.

use serde::de::DeserializeOwned;
use serde_reflection::{Tracer, TracerConfig};

use crate::ipc::{Request, Response};

/// A hash of `T`'s encoding: the shape of every type it contains, as `serde`
/// sees them in a binary format. Two builds agree on it exactly when they
/// encode `T` the same way.
///
/// The registry is taken unchecked: tracing visits every variant of an enum by
/// name, in declaration order, but only resolves the variant indices of the
/// enums traced directly. Names, contents and order are all a fingerprint
/// needs, and nested enums would otherwise each have to be traced by hand.
pub(crate) fn fingerprint<T: DeserializeOwned>() -> Result<u64, String> {
    let mut tracer = Tracer::new(TracerConfig::default());
    let (root, _) = tracer.trace_simple_type::<T>().map_err(|e| e.to_string())?;
    let registry = tracer.registry_unchecked();
    let bytes = postcard::to_allocvec(&(root, registry)).map_err(|e| e.to_string())?;
    Ok(fnv1a(&bytes))
}

/// The protocol's fingerprint: what a component must have been built with for
/// the subprocess to talk to it.
pub(crate) fn wire() -> Result<u64, String> {
    Ok(fnv1a(
        &[
            fingerprint::<Request>()?.to_le_bytes(),
            fingerprint::<Response>()?.to_le_bytes(),
        ]
        .concat(),
    ))
}

/// FNV-1a's starting value for 64-bit hashes, fixed by its specification:
/// what any implementation starts from, so hashes agree.
const FNV_OFFSET_BASIS: u64 = 0xCBF2_9CE4_8422_2325;
/// FNV's 64-bit prime, fixed by its specification: 2^40 + 2^8 + 0xb3, chosen
/// by FNV's authors to spread the bits of each byte across the hash.
const FNV_PRIME: u64 = 0x0100_0000_01B3;

/// 64-bit FNV-1a. Not `std`'s hasher, whose algorithm may change between Rust
/// releases: a fingerprint has to mean the same thing to every build.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(FNV_OFFSET_BASIS, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME))
}

#[cfg(test)]
mod tests;
