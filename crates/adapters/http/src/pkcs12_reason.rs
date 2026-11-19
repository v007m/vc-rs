//! Why a PKCS#12 bundle was refused, from `p12-keystore`'s error and the bundle's layout.

use std::fmt;

use der::asn1::AnyRef;
use der::{Decode, Reader, SliceReader, Tag, Tagged};
use p12_keystore::error::Error as P12Error;

/// Why a bundle was refused. [`fmt::Display`] gives the message, which never quotes the bundle,
/// the passphrase or `p12-keystore`'s text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// The passphrase has a character PKCS#12's `BMPString` cannot hold: U+FFFF or above.
    NonBmpPassphrase,
    /// The MAC does not match.
    WrongPassphrase,
    /// A bundle without a MAC failed in an encrypted part, as a wrong passphrase does.
    Undecryptable,
    /// The MAC matched, but an encrypted part failed.
    BagUndecryptable,
    /// A cipher, MAC or PBES2 scheme `p12-keystore` does not support.
    Unsupported,
    /// Key-derivation parameters over `p12-keystore`'s limits, or missing.
    OverLimit,
    /// The private key is not PKCS#8.
    NotPkcs8,
    /// The private key is PKCS#8 of a type ring cannot sign with.
    KeyType,
    /// A certificate is not X.509.
    NotX509,
    /// The file is not PKCS#12.
    NotPkcs12,
    /// The bundle has no private key.
    NoKey,
    /// The bundle has this many private keys, more than one.
    Keys(usize),
    /// The bundle has no certificate.
    NoCertificate,
    /// No certificate's public key is the private key's.
    NoMatchingCertificate,
}

const EXPORT_AGAIN: &str = "export it again with AES \
                            (`openssl pkcs12 -export -keypbe AES-256-CBC -certpbe AES-256-CBC -macalg sha256`)";

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonBmpPassphrase => f.write_str(
                "the passphrase has a character PKCS#12 cannot encode (one outside the Basic Multilingual \
                 Plane, such as an emoji, or U+FFFF)",
            ),
            Self::WrongPassphrase => {
                f.write_str("the passphrase is wrong, or the bundle is damaged (its MAC does not match)")
            }
            Self::Undecryptable => f.write_str("could not decrypt the bundle: check the passphrase"),
            Self::BagUndecryptable => write!(
                f,
                "could not decrypt a bag: the bundle may use a separate encryption password (`-twopass`), be \
                 damaged, or use an unsupported cipher; {EXPORT_AGAIN}"
            ),
            Self::Unsupported => write!(f, "the bundle uses a cipher or MAC that is not supported; {EXPORT_AGAIN}"),
            Self::OverLimit => write!(
                f,
                "unsupported or over-limit key-derivation parameters (iterations, scrypt cost or missing \
                 parameters); {EXPORT_AGAIN}"
            ),
            Self::NotPkcs8 => f.write_str("the bundle's private key is not valid PKCS#8"),
            Self::KeyType => f.write_str(
                "the bundle's private key is of a type that is not supported (use RSA, ECDSA P-256 or P-384, \
                 or Ed25519)",
            ),
            Self::NotX509 => f.write_str("the bundle holds a certificate that is not valid X.509"),
            Self::NotPkcs12 => f.write_str("the file is not a valid PKCS#12 bundle"),
            Self::NoKey => f.write_str("the bundle has no private key"),
            Self::Keys(count) => write!(f, "the bundle holds {count} private keys; export only the one to send"),
            Self::NoCertificate => f.write_str("the bundle has no certificate"),
            Self::NoMatchingCertificate => f.write_str("no certificate in the bundle matches its private key"),
        }
    }
}

impl std::error::Error for Reason {}

impl Reason {
    /// The reason for `p12-keystore`'s error. A failure that decrypting may have caused gets
    /// [`Self::Undecryptable`] (check the passphrase) without a MAC, [`Self::BagUndecryptable`]
    /// with one; any other failure names what is wrong.
    pub const fn of(error: &P12Error, layout: Layout) -> Self {
        match error {
            P12Error::MacError(_) => Self::WrongPassphrase,
            P12Error::InvalidParameters => Self::OverLimit,
            P12Error::UnsupportedEncryptionScheme | P12Error::UnsupportedMacAlgorithm => Self::Unsupported,
            _ if layout.may_have_garbled(error) => {
                if layout.mac {
                    Self::BagUndecryptable
                } else {
                    Self::Undecryptable
                }
            }
            P12Error::InvalidPrivateKey => Self::NotPkcs8,
            P12Error::X509Error(_) | P12Error::UnsupportedCertificateType => Self::NotX509,
            _ => Self::NotPkcs12,
        }
    }
}

/// What a bundle holds, read from its DER as far as it goes: whether it has a MAC (assumed when
/// the outer `PFX` cannot be read), an `EncryptedData` safe, or a shrouded key bag (RFC 7292).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub mac: bool,
    pub encrypted_safe: bool,
    pub shrouded_key: bool,
}

const DATA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x01];
const ENCRYPTED_DATA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x06];
const SHROUDED_KEY_BAG: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x0C, 0x0A, 0x01, 0x02];

impl Layout {
    /// The layout of `bytes`. What could not be read counts as absent, except the MAC.
    pub fn of(bytes: &[u8]) -> Self {
        let mut layout = Self { mac: true, encrypted_safe: false, shrouded_key: false };
        let _partial = layout.read(bytes);
        layout
    }

    /// Whether `error` may come from decrypting with the wrong key: always for a failed unpad or
    /// PBES2 decryption, and for a key or certificate that does not parse when it was encrypted.
    const fn may_have_garbled(self, error: &P12Error) -> bool {
        match error {
            P12Error::Pkcs5Error(_) | P12Error::UnpadError => true,
            P12Error::X509Error(_) | P12Error::UnsupportedCertificateType => self.encrypted_safe,
            P12Error::InvalidPrivateKey | P12Error::DerError(_) | P12Error::InvalidData => {
                self.encrypted_safe || self.shrouded_key
            }
            _ => false,
        }
    }

    /// `PFX ::= SEQUENCE { version, authSafe ContentInfo, macData OPTIONAL }`, then each safe.
    fn read(&mut self, bytes: &[u8]) -> der::Result<()> {
        let outer = AnyRef::from_der(bytes)?;
        let mut fields = SliceReader::new(outer.value())?;
        let _version: AnyRef<'_> = fields.decode()?;
        let auth_safe: AnyRef<'_> = fields.decode()?;
        self.mac = outer.tag() != Tag::Sequence || !fields.is_finished();
        let (_, safes) = content_info(auth_safe)?;
        each(octets(safes)?, |safe| {
            let (kind, content) = content_info(safe)?;
            self.encrypted_safe |= kind == ENCRYPTED_DATA;
            if kind != DATA {
                return Ok(());
            }
            each(octets(content)?, |bag| {
                self.shrouded_key |= content_info(bag)?.0 == SHROUDED_KEY_BAG;
                Ok(())
            })
        })
    }
}

/// A `ContentInfo` or `SafeBag`: its type's OID bytes and its `[0]` content.
fn content_info(info: AnyRef<'_>) -> der::Result<(&[u8], AnyRef<'_>)> {
    let mut fields = SliceReader::new(info.value())?;
    let kind: AnyRef<'_> = fields.decode()?;
    let content: AnyRef<'_> = fields.decode()?;
    Ok((kind.value(), content))
}

/// The bytes of the `OCTET STRING` inside a `[0]` content.
fn octets(content: AnyRef<'_>) -> der::Result<&[u8]> {
    Ok(AnyRef::from_der(content.value())?.value())
}

/// Calls `visit` on each element of the `SEQUENCE OF` in `bytes`.
fn each<'a>(bytes: &'a [u8], mut visit: impl FnMut(AnyRef<'a>) -> der::Result<()>) -> der::Result<()> {
    let sequence = AnyRef::from_der(bytes)?;
    let mut elements = SliceReader::new(sequence.value())?;
    while !elements.is_finished() {
        visit(elements.decode()?)?;
    }
    Ok(())
}
