//! Sealing what goes through the relay: ChaCha20-Poly1305, with a key and a
//! topic made from a code (a friend code, a village code) by SHA-256. Only
//! someone with the code can read what's sealed with it, or even tell which
//! topic it's on. Also the codes themselves, and the randomness they need.

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};

fn digest(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update((p.len() as u32).to_le_bytes());
        h.update(p);
    }
    h.finalize().into()
}

/// A code as typed: capitals, letters and digits only ("snow-7q2k" and
/// "SNOW7Q2K" are the same code).
pub fn norm(code: &str) -> String {
    code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect()
}

/// The topic for one kind of thing (a mailbox, a "here I am", a village)
/// keyed by a code.
pub fn topic(kind: &str, code: &str) -> String {
    let d = digest(&[b"xmas-tui topic", kind.as_bytes(), norm(code).as_bytes()]);
    let hex: String = d[..12].iter().map(|b| format!("{b:02x}")).collect();
    format!("xmas-tui/1/{kind}/{hex}")
}

static POOL: AtomicU64 = AtomicU64::new(0x9e37_79b9_7f4a_7c15);
static COUNT: AtomicU64 = AtomicU64::new(0);

/// Stir some randomness into the pool (the web page's crypto, the clock).
pub fn stir(v: u64) {
    let old = POOL.load(Ordering::Relaxed);
    POOL.store(mix(old ^ v.rotate_left(17) ^ 0xd1b5_4a32_d192_ed03), Ordering::Relaxed);
}

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A random number: the pool, the system's own randomness where there is
/// any, the time, a counter.
pub fn random() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(COUNT.fetch_add(1, Ordering::Relaxed));
    let sys = h.finish();
    let now = crate::clock::now().timestamp_nanos_opt().unwrap_or(0) as u64;
    let v = mix(POOL.load(Ordering::Relaxed) ^ sys ^ now.rotate_left(29) ^ COUNT.load(Ordering::Relaxed).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    stir(v);
    v
}

const LETTERS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// A new code, "XXXX-XXXX-XXXX" (no I, O, 0 or 1 to mix up).
pub fn new_code() -> String {
    let (a, b) = (random(), random());
    let mut s = String::new();
    for i in 0..12 {
        if i > 0 && i % 4 == 0 {
            s.push('-');
        }
        let v = if i < 6 { a >> (i * 10) } else { b >> ((i - 6) * 10) };
        s.push(LETTERS[(v % LETTERS.len() as u64) as usize] as char);
    }
    s
}

/// A code shown as typed ("XXXX-XXXX-XXXX").
pub fn pretty(code: &str) -> String {
    let n = norm(code);
    n.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).to_string()).collect::<Vec<_>>().join("-")
}

/// Sealing and opening with one code's key.
#[derive(Clone)]
pub struct Seal {
    cipher: ChaCha20Poly1305,
}

impl Seal {
    pub fn new(code: &str) -> Seal {
        let key = digest(&[b"xmas-tui key", norm(code).as_bytes()]);
        Seal { cipher: ChaCha20Poly1305::new(Key::from_slice(&key)) }
    }

    /// A fresh nonce, then the sealed text.
    pub fn seal(&self, plain: &[u8]) -> Vec<u8> {
        let mut nonce = [0u8; 12];
        nonce[..8].copy_from_slice(&random().to_le_bytes());
        nonce[8..].copy_from_slice(&(random() as u32).to_le_bytes());
        let mut out = nonce.to_vec();
        out.extend(self.cipher.encrypt(Nonce::from_slice(&nonce), plain).unwrap_or_default());
        out
    }

    /// What was sealed (None: not sealed with this code, or tampered with).
    pub fn open(&self, data: &[u8]) -> Option<Vec<u8>> {
        if data.len() < 12 + 16 {
            return None;
        }
        self.cipher.decrypt(Nonce::from_slice(&data[..12]), &data[12..]).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_code_opens_what_it_sealed() {
        let s = Seal::new("SNOW-7Q2K-91XZ");
        let sealed = s.seal(b"meet by the tree");
        assert_ne!(&sealed[12..], b"meet by the tree");
        assert_eq!(Seal::new("snow 7q2k 91xz").open(&sealed).as_deref(), Some(&b"meet by the tree"[..]));
        assert!(Seal::new("SNOW-7Q2K-91XY").open(&sealed).is_none());
        // the same words sealed twice look different
        assert_ne!(s.seal(b"hi"), s.seal(b"hi"));
        // codes: twelve letters and digits, none to mix up, and different
        let (a, b) = (new_code(), new_code());
        assert_ne!(a, b);
        assert_eq!(norm(&a).len(), 12);
        assert!(!a.contains(['I', 'O', '0', '1']));
        assert_eq!(pretty(&norm(&a)), a);
        assert_eq!(topic("here", &a), topic("here", &a.to_lowercase()));
        assert_ne!(topic("here", &a), topic("mail", &a));
    }
}
