use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::error::{CoreError, Result};

pub fn decode_b64_32(s: &str) -> Result<[u8; 32]> {
    let raw = B64.decode(s.trim().as_bytes())?;
    if raw.len() != 32 {
        return Err(CoreError::PublicKey);
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&raw);
    Ok(out)
}

pub fn verify_manifest_sig_b64(
    manifest_json_bytes: &[u8],
    manifest_sig_b64: &str,
    public_key_b64: &str,
) -> Result<()> {
    let sig_raw = B64.decode(manifest_sig_b64.trim().as_bytes())?;
    let sig = Signature::from_slice(&sig_raw).map_err(|_| CoreError::Signature)?;

    let pk = decode_b64_32(public_key_b64)?;
    let vk = VerifyingKey::from_bytes(&pk).map_err(|_| CoreError::PublicKey)?;

    vk.verify_strict(manifest_json_bytes, &sig)
        .map_err(|_| CoreError::Signature)?;

    Ok(())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex_lower(&digest)
}

fn hex_lower(bytes: &[u8]) -> String {
    const LUT: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(LUT[(b >> 4) as usize] as char);
        out.push(LUT[(b & 0x0f) as usize] as char);
    }
    out
}
