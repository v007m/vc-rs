use std::fmt;
use std::fmt::Write as _;

use chacha20poly1305::aead::{Aead, Generate, Payload};
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
use zeroize::Zeroizing;

/// The bytes of a nonce, which lead every sealed value.
const NONCE_LEN: usize = 12;

/// The cache's encryption key: 32 bytes, from the OS keychain or `VIB_CACHE_KEY`. It is wiped
/// when dropped and never printed.
#[derive(Clone)]
pub struct CacheSecret(Zeroizing<[u8; 32]>);

impl CacheSecret {
    /// The key with these bytes.
    #[must_use]
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// A new key from the OS's random source, for a keychain that holds none yet; `None` when
    /// the OS gives no random bytes.
    #[must_use]
    pub fn generate() -> Option<Self> {
        let key = chacha20poly1305::Key::try_generate().ok()?;
        Some(Self::new(key.into()))
    }

    /// The key written as 64 hex digits, as `VIB_CACHE_KEY` and the keychain hold it; `None`
    /// for anything else.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim().as_bytes();
        if text.len() != 64 {
            return None;
        }
        let mut bytes = Zeroizing::new([0_u8; 32]);
        let (pairs, _) = text.as_chunks::<2>();
        for (byte, [high, low]) in bytes.iter_mut().zip(pairs) {
            *byte = (hex(*high)? << 4) | hex(*low)?;
        }
        Some(Self(bytes))
    }

    /// The key as 64 hex digits, for the keychain.
    #[must_use]
    pub fn to_hex(&self) -> Zeroizing<String> {
        let mut text = Zeroizing::new(String::with_capacity(64));
        for byte in self.0.iter() {
            let _ = write!(text, "{byte:02x}");
        }
        text
    }
}

impl fmt::Debug for CacheSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CacheSecret(••••)")
    }
}

fn hex(digit: u8) -> Option<u8> {
    char::from(digit).to_digit(16).and_then(|value| u8::try_from(value).ok())
}

/// Seals and opens cache values with ChaCha20-Poly1305. The nonce is synthetic: a keyed blake3
/// hash of the associated data and the plain text, so it never repeats for different values
/// under one key without needing a random source; equal values seal equally, which only shows
/// that two rows hold the same thing.
pub struct Sealer {
    cipher: ChaCha20Poly1305,
    nonce_key: Zeroizing<[u8; 32]>,
    tag_key: Zeroizing<[u8; 32]>,
}

impl Sealer {
    /// A sealer for `secret`.
    pub fn new(secret: &CacheSecret) -> Self {
        let nonce_key = Zeroizing::new(blake3::derive_key("vibranium cache.db nonce v1", secret.0.as_slice()));
        let tag_key = Zeroizing::new(blake3::derive_key("vibranium cache.db tag v1", secret.0.as_slice()));
        Self { cipher: ChaCha20Poly1305::new(&(*secret.0).into()), nonce_key, tag_key }
    }

    /// A keyed blake3 hash of `parts`, each length-prefixed: equal only for equal parts, and
    /// telling nothing about them without the key.
    pub fn tag(&self, parts: &[&[u8]]) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new_keyed(&self.tag_key);
        for part in parts {
            hasher.update(&u64::try_from(part.len()).unwrap_or(u64::MAX).to_le_bytes()).update(part);
        }
        *hasher.finalize().as_bytes()
    }

    /// `plain`, bound to `aad`, as nonce then cipher text; `None` only if the cipher refuses.
    pub fn seal(&self, aad: &[u8], plain: &[u8]) -> Option<Vec<u8>> {
        let mut hasher = blake3::Hasher::new_keyed(&self.nonce_key);
        hasher.update(&u64::try_from(aad.len()).unwrap_or(u64::MAX).to_le_bytes()).update(aad).update(plain);
        let mut nonce = [0_u8; NONCE_LEN];
        hasher.finalize_xof().fill(&mut nonce);
        let sealed = self.cipher.encrypt(&Nonce::from(nonce), Payload { msg: plain, aad }).ok()?;
        Some([nonce.as_slice(), &sealed].concat())
    }

    /// The plain text `sealed` holds; `None` when it was sealed under another key or `aad`, or
    /// was changed since.
    pub fn open(&self, aad: &[u8], sealed: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
        let (nonce, cipher_text) = sealed.split_at_checked(NONCE_LEN)?;
        let nonce: [u8; NONCE_LEN] = nonce.try_into().ok()?;
        self.cipher.decrypt(&Nonce::from(nonce), Payload { msg: cipher_text, aad }).ok().map(Zeroizing::new)
    }
}

#[cfg(test)]
mod tests {
    use super::{CacheSecret, Sealer};

    const HEX: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

    #[test]
    fn keys_are_64_hex_digits() {
        let secret = CacheSecret::parse(HEX);
        assert_eq!(secret.as_ref().map(|secret| secret.to_hex().to_string()).as_deref(), Some(HEX));
        assert_eq!(
            CacheSecret::parse(&HEX.to_uppercase()).map(|secret| secret.to_hex().to_string()).as_deref(),
            Some(HEX)
        );
        assert!(CacheSecret::parse(&HEX[2..]).is_none(), "too short");
        assert!(CacheSecret::parse(&format!("zz{}", &HEX[2..])).is_none(), "not hex");
        assert_eq!(format!("{:?}", CacheSecret::new([7; 32])), "CacheSecret(••••)");
    }

    #[test]
    fn opens_only_what_it_sealed_under_the_same_key_and_data() {
        let crypt = Sealer::new(&CacheSecret::new([1; 32]));
        let sealed = crypt.seal(b"row", b"tok-secret").unwrap_or_default();
        assert!(!sealed.windows(10).any(|window| window == b"tok-secret"), "nothing in the clear");
        assert_eq!(crypt.open(b"row", &sealed).map(|plain| plain.to_vec()), Some(b"tok-secret".to_vec()));
        assert_eq!(crypt.open(b"other row", &sealed), None, "bound to its row");
        assert_eq!(Sealer::new(&CacheSecret::new([2; 32])).open(b"row", &sealed), None, "another key");
        let mut changed = sealed.clone();
        if let Some(last) = changed.last_mut() {
            *last ^= 1;
        }
        assert_eq!(crypt.open(b"row", &changed), None, "changed");
        assert_eq!(crypt.open(b"row", &sealed[..5]), None, "too short");
    }

    #[test]
    fn different_values_get_different_nonces() {
        let sealer = Sealer::new(&CacheSecret::new([1; 32]));
        let first = sealer.seal(b"row", b"a").unwrap_or_default();
        let second = sealer.seal(b"row", b"b").unwrap_or_default();
        assert_ne!(first.get(..12), second.get(..12));
        assert_eq!(sealer.seal(b"row", b"a"), Some(first));
    }
}
