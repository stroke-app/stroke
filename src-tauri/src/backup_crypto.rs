//! End-to-end encryption for scheduled backups.
//!
//! A backup is encrypted on the user's computer before it is uploaded, so
//! Stroke Cloud (and anyone with access to it) only ever holds ciphertext.
//!
//! - **Data key (DEK):** 32 random bytes, fresh for every backup.
//! - **Wrapped DEK:** `nonce24 || XChaCha20-Poly1305(sync_key, nonce, dek,
//!   aad = "stroke-backup-dek-v1")`, 72 bytes. The sync key is the account's
//!   end-to-end sync key (`sync.rs`), unlocked only by the sync passphrase.
//! - **Key id:** the first 8 bytes of SHA-256("stroke-backup-key-id" ||
//!   sync_key), hex. Says which sync key wrapped the DEK without revealing it.
//! - **File:** `"STRKBAK1" || key_id(8 raw) || wrapped_dek(72) ||
//!   nonce_prefix(19)` (107 bytes), then the plaintext in 64 KiB chunks, each
//!   `XChaCha20-Poly1305(dek, prefix || counter_u32_be || last_flag, chunk,
//!   aad = the 107-byte header)`. The last chunk has flag 1, so a truncated or
//!   reordered file fails to open. An empty input is one empty last chunk.
//!
//! The browser decrypts the same format (stroke-web
//! `src/lib/backups/crypto.ts`); keep the two in step.

use std::io::{Read, Write};
use std::path::Path;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use sha2::{Digest, Sha256};

pub const MAGIC: &[u8; 8] = b"STRKBAK1";
const DEK_AAD: &[u8] = b"stroke-backup-dek-v1";
pub const CHUNK: usize = 64 * 1024;
const TAG: usize = 16;
const WRAPPED_LEN: usize = 24 + 32 + TAG;
pub const HEADER_LEN: usize = 8 + 8 + WRAPPED_LEN + 19;

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("the OS random source is available");
    b
}

/// Which sync key wrapped a backup's data key, as 16 hex characters.
pub fn key_id(sync_key: &[u8; 32]) -> String {
    hex::encode(&key_id_raw(sync_key))
}

fn key_id_raw(sync_key: &[u8; 32]) -> [u8; 8] {
    let mut h = Sha256::new();
    h.update(b"stroke-backup-key-id");
    h.update(sync_key);
    let d = h.finalize();
    d[..8].try_into().expect("8 of 32 bytes")
}

pub fn wrap_dek(sync_key: &[u8; 32], dek: &[u8; 32]) -> Vec<u8> {
    let nonce: [u8; 24] = random();
    let ct = XChaCha20Poly1305::new(sync_key.into())
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: dek, aad: DEK_AAD })
        .expect("in-memory encryption cannot fail");
    [nonce.as_slice(), &ct].concat()
}

pub fn unwrap_dek(sync_key: &[u8; 32], wrapped: &[u8]) -> Result<[u8; 32], String> {
    if wrapped.len() != WRAPPED_LEN {
        return Err("The backup's key is damaged.".into());
    }
    let (nonce, ct) = wrapped.split_at(24);
    let dek = XChaCha20Poly1305::new(sync_key.into())
        .decrypt(XNonce::from_slice(nonce), Payload { msg: ct, aad: DEK_AAD })
        .map_err(|_| "This backup was encrypted with another sync key. Unlock sync with the passphrase it was made under.".to_string())?;
    dek.try_into().map_err(|_| "The backup's key is damaged.".to_string())
}

/// Re-wrap a data key under a new sync key (after a sync reset on a device
/// that still had the old key).
pub fn rewrap(old: &[u8; 32], new: &[u8; 32], wrapped: &[u8]) -> Result<Vec<u8>, String> {
    Ok(wrap_dek(new, &unwrap_dek(old, wrapped)?))
}

fn chunk_nonce(prefix: &[u8; 19], counter: u32, last: bool) -> [u8; 24] {
    let mut n = [0u8; 24];
    n[..19].copy_from_slice(prefix);
    n[19..23].copy_from_slice(&counter.to_be_bytes());
    n[23] = u8::from(last);
    n
}

/// Read until `buf` is full or the input ends; returns how much was read.
fn fill(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

/// What the server keeps beside an encrypted backup.
#[derive(Debug, Clone)]
pub struct Sealed {
    pub key_id: String,
    /// base64url (unpadded) of the 72-byte wrapped data key.
    pub wrapped_key: String,
}

pub fn b64(bytes: &[u8]) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn unb64(s: &str) -> Result<Vec<u8>, String> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    URL_SAFE_NO_PAD.decode(s.trim_end_matches('=')).map_err(|e| e.to_string())
}

pub fn encrypt_stream(sync_key: &[u8; 32], mut input: impl Read, mut out: impl Write) -> Result<Sealed, String> {
    let dek: [u8; 32] = random();
    let wrapped = wrap_dek(sync_key, &dek);
    let prefix: [u8; 19] = random();
    let mut header = Vec::with_capacity(HEADER_LEN);
    header.extend_from_slice(MAGIC);
    header.extend_from_slice(&key_id_raw(sync_key));
    header.extend_from_slice(&wrapped);
    header.extend_from_slice(&prefix);
    out.write_all(&header).map_err(|e| e.to_string())?;

    let cipher = XChaCha20Poly1305::new((&dek).into());
    let mut cur = vec![0u8; CHUNK];
    let mut next = vec![0u8; CHUNK];
    let mut cur_len = fill(&mut input, &mut cur).map_err(|e| e.to_string())?;
    let mut counter: u32 = 0;
    loop {
        // Look one chunk ahead: the last chunk is the one with nothing after it.
        let next_len = if cur_len == CHUNK { fill(&mut input, &mut next).map_err(|e| e.to_string())? } else { 0 };
        let last = next_len == 0;
        let nonce = chunk_nonce(&prefix, counter, last);
        let ct = cipher
            .encrypt(XNonce::from_slice(&nonce), Payload { msg: &cur[..cur_len], aad: &header })
            .map_err(|_| "Encryption failed.".to_string())?;
        out.write_all(&ct).map_err(|e| e.to_string())?;
        if last {
            break;
        }
        counter = counter.checked_add(1).ok_or("The backup is too large to encrypt.")?;
        std::mem::swap(&mut cur, &mut next);
        cur_len = next_len;
    }
    out.flush().map_err(|e| e.to_string())?;
    Ok(Sealed { key_id: key_id(sync_key), wrapped_key: b64(&wrapped) })
}

/// Decrypt a backup. `wrapped_override` is the server's copy of the wrapped
/// key, which may have been re-wrapped under a newer sync key since the file
/// was written.
pub fn decrypt_stream(
    sync_key: &[u8; 32],
    mut input: impl Read,
    mut out: impl Write,
    wrapped_override: Option<&[u8]>,
) -> Result<(), String> {
    let mut header = [0u8; HEADER_LEN];
    if fill(&mut input, &mut header).map_err(|e| e.to_string())? != HEADER_LEN || &header[..8] != MAGIC {
        return Err("This isn't an encrypted Stroke backup.".into());
    }
    let wrapped = wrapped_override.unwrap_or(&header[16..16 + WRAPPED_LEN]);
    let dek = unwrap_dek(sync_key, wrapped)?;
    let prefix: [u8; 19] = header[16 + WRAPPED_LEN..].try_into().expect("19 bytes");
    let cipher = XChaCha20Poly1305::new((&dek).into());

    let size = CHUNK + TAG;
    let mut cur = vec![0u8; size];
    let mut next = vec![0u8; size];
    let mut cur_len = fill(&mut input, &mut cur).map_err(|e| e.to_string())?;
    let mut counter: u32 = 0;
    loop {
        let next_len = if cur_len == size { fill(&mut input, &mut next).map_err(|e| e.to_string())? } else { 0 };
        let last = next_len == 0;
        let nonce = chunk_nonce(&prefix, counter, last);
        let pt = cipher
            .decrypt(XNonce::from_slice(&nonce), Payload { msg: &cur[..cur_len], aad: &header })
            .map_err(|_| "The backup is damaged or incomplete.".to_string())?;
        out.write_all(&pt).map_err(|e| e.to_string())?;
        if last {
            break;
        }
        counter = counter.checked_add(1).ok_or("The backup is damaged.")?;
        std::mem::swap(&mut cur, &mut next);
        cur_len = next_len;
    }
    out.flush().map_err(|e| e.to_string())
}

pub fn encrypt_file(sync_key: &[u8; 32], src: &Path, dst: &Path) -> Result<Sealed, String> {
    let input = std::io::BufReader::new(std::fs::File::open(src).map_err(|e| e.to_string())?);
    let out = std::io::BufWriter::new(std::fs::File::create(dst).map_err(|e| e.to_string())?);
    encrypt_stream(sync_key, input, out)
}

pub fn is_encrypted(head: &[u8]) -> bool {
    head.len() >= 8 && &head[..8] == MAGIC
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(len: usize) {
        let key = [7u8; 32];
        let data: Vec<u8> = (0..len).map(|i| (i * 31 % 251) as u8).collect();
        let mut enc = Vec::new();
        let sealed = encrypt_stream(&key, data.as_slice(), &mut enc).unwrap();
        assert_eq!(sealed.key_id, key_id(&key));
        assert!(is_encrypted(&enc));
        let mut dec = Vec::new();
        decrypt_stream(&key, enc.as_slice(), &mut dec, None).unwrap();
        assert_eq!(dec, data, "len {len}");
    }

    #[test]
    fn round_trips_at_chunk_edges() {
        for len in [0, 1, CHUNK - 1, CHUNK, CHUNK + 1, 3 * CHUNK, 3 * CHUNK + 17] {
            round_trip(len);
        }
    }

    #[test]
    fn wrong_key_truncation_and_tampering_fail() {
        let key = [1u8; 32];
        let data = vec![9u8; 2 * CHUNK + 5];
        let mut enc = Vec::new();
        encrypt_stream(&key, data.as_slice(), &mut enc).unwrap();
        assert!(decrypt_stream(&[2u8; 32], enc.as_slice(), Vec::new(), None).is_err());
        // Cut off at a chunk boundary: the new last chunk was not sealed as last.
        let cut = &enc[..HEADER_LEN + 2 * (CHUNK + TAG)];
        assert!(decrypt_stream(&key, cut, Vec::new(), None).is_err());
        let mut bad = enc.clone();
        let n = bad.len();
        bad[n - 20] ^= 1;
        assert!(decrypt_stream(&key, bad.as_slice(), Vec::new(), None).is_err());
        let mut bad_header = enc.clone();
        bad_header[HEADER_LEN - 1] ^= 1;
        assert!(decrypt_stream(&key, bad_header.as_slice(), Vec::new(), None).is_err());
    }

    #[test]
    fn rewrapped_key_opens_with_the_new_sync_key() {
        let (old, new) = ([3u8; 32], [4u8; 32]);
        let data = b"CREATE TABLE t (x int);".to_vec();
        let mut enc = Vec::new();
        let sealed = encrypt_stream(&old, data.as_slice(), &mut enc).unwrap();
        let re = rewrap(&old, &new, &unb64(&sealed.wrapped_key).unwrap()).unwrap();
        let mut dec = Vec::new();
        decrypt_stream(&new, enc.as_slice(), &mut dec, Some(&re)).unwrap();
        assert_eq!(dec, data);
        assert!(decrypt_stream(&new, enc.as_slice(), Vec::new(), None).is_err());
    }

    /// A fixed vector the browser's decryptor is tested against.
    #[test]
    fn known_answer_for_the_web_decryptor() {
        let key = [0x42u8; 32];
        let dek = [0x11u8; 32];
        let prefix = [0x22u8; 19];
        let wrap_nonce = [0x33u8; 24];
        let wrapped = [
            wrap_nonce.as_slice(),
            &XChaCha20Poly1305::new((&key).into())
                .encrypt(XNonce::from_slice(&wrap_nonce), Payload { msg: &dek, aad: DEK_AAD })
                .unwrap(),
        ]
        .concat();
        let mut header = Vec::new();
        header.extend_from_slice(MAGIC);
        header.extend_from_slice(&key_id_raw(&key));
        header.extend_from_slice(&wrapped);
        header.extend_from_slice(&prefix);
        let ct = XChaCha20Poly1305::new((&dek).into())
            .encrypt(XNonce::from_slice(&chunk_nonce(&prefix, 0, true)), Payload { msg: b"hello", aad: &header })
            .unwrap();
        let file = [header.as_slice(), &ct].concat();
        let mut dec = Vec::new();
        decrypt_stream(&key, file.as_slice(), &mut dec, None).unwrap();
        assert_eq!(dec, b"hello");
        // Printed for the web test: cargo test known_answer -- --nocapture
        println!("KAT {}", b64(&file));
    }
}
