//! PKCS#12 bundles (D352), decoded with `p12-keystore` into a DER chain and a PKCS#8 key.

use p12_keystore::error::Error as P12Error;
use p12_keystore::{CertificateBag, LocalKeyId, Pkcs12Archive};
use rustls::pki_types::CertificateDer;
use zeroize::Zeroizing;

/// A decoded bundle: the chain, leaf first, and the PKCS#8 key in memory wiped on drop.
pub struct Decoded {
    pub chain: Vec<CertificateDer<'static>>,
    pub key: Zeroizing<Vec<u8>>,
}

/// Decodes a bundle that holds one private key and its certificate, or says what is wrong
/// without quoting the bundle or the passphrase.
pub fn decode(bytes: &[u8], password: &str) -> Result<Decoded, String> {
    let archive = Pkcs12Archive::from_pkcs12(bytes, password).map_err(|error| reason(&error).to_owned())?;
    let [bag] = archive.keys.as_slice() else {
        return Err(match archive.keys.len() {
            0 => "the bundle has no private key".to_owned(),
            count => format!("the bundle holds {count} private keys; export only the one to send"),
        });
    };
    if archive.certs.is_empty() {
        return Err("the bundle has no certificate".to_owned());
    }
    let key = Zeroizing::new(bag.key.as_der().to_vec());
    Ok(Decoded { chain: chain(&archive.certs, bag.local_key_id.as_ref()), key })
}

/// The certificates, the one whose local key ID is the key's first, then the rest in bag order.
fn chain(certs: &[CertificateBag], key_id: Option<&LocalKeyId>) -> Vec<CertificateDer<'static>> {
    let owns = |bag: &&CertificateBag| key_id.is_some_and(|id| bag.local_key_id.as_deref() == Some(id.as_ref()));
    let (mut ordered, rest): (Vec<_>, Vec<_>) = certs.iter().partition(owns);
    ordered.extend(rest);
    ordered.into_iter().map(|bag| CertificateDer::from(bag.cert.as_der().to_vec())).collect()
}

const UNSUPPORTED: &str = "the bundle uses a cipher or MAC that is not supported; export it again with AES \
                           (`openssl pkcs12 -export -keypbe AES-256-CBC -certpbe AES-256-CBC -macalg sha256`)";

/// What is wrong with a bundle, from `p12-keystore`'s error, without its text.
const fn reason(error: &P12Error) -> &'static str {
    match error {
        P12Error::MacError(_) => "the passphrase is wrong, or the bundle is damaged (its MAC does not match)",
        P12Error::Pkcs5Error(_) | P12Error::UnpadError => {
            "the bundle could not be decrypted: the passphrase is wrong or its cipher is not supported"
        }
        P12Error::UnsupportedEncryptionScheme | P12Error::UnsupportedMacAlgorithm => UNSUPPORTED,
        P12Error::InvalidParameters => "the bundle's key derivation uses more iterations than are allowed",
        P12Error::InvalidPrivateKey => "the bundle's private key is not valid PKCS#8",
        P12Error::X509Error(_) | P12Error::UnsupportedCertificateType => {
            "the bundle holds a certificate that is not valid X.509"
        }
        _ => "the file is not a valid PKCS#12 bundle",
    }
}

#[cfg(test)]
mod tests {
    use p12_keystore::error::Error as P12Error;
    use p12_keystore::{Certificate, KeyStore, KeyStoreEntry, PrivateKey, PrivateKeyChain};
    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};

    use super::{decode, reason};

    type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

    const RSA_CERT: &[u8] = include_bytes!("../tests/fixtures/rsa.pem");
    const RSA_KEY: &[u8] = include_bytes!("../tests/fixtures/rsa.pkcs8.key");

    fn reason_for(bytes: &[u8], password: &str) -> String {
        decode(bytes, password).err().unwrap_or_else(|| "decoded".to_owned())
    }

    fn parts() -> TestResult<(Certificate, PrivateKey)> {
        let cert = CertificateDer::from_pem_slice(RSA_CERT)?;
        let key = PrivateKeyDer::from_pem_slice(RSA_KEY)?;
        Ok((Certificate::from_der(&cert)?, PrivateKey::from_der(key.secret_der())?))
    }

    fn bundle(entries: Vec<KeyStoreEntry>) -> TestResult<Vec<u8>> {
        let mut store = KeyStore::new();
        for (index, entry) in entries.into_iter().enumerate() {
            store.add_entry(&format!("entry{index}"), entry);
        }
        Ok(store.writer("pw").write()?)
    }

    #[test]
    fn decodes_openssl_bundles_with_aes_and_with_3des_and_rc2() -> Result<(), String> {
        for bytes in
            [&include_bytes!("../tests/fixtures/ec.p12")[..], include_bytes!("../tests/fixtures/ec.legacy.p12")]
        {
            let decoded = decode(bytes, "fixture")?;
            assert_eq!(decoded.chain.len(), 1);
            assert!(!decoded.key.is_empty());
        }
        Ok(())
    }

    #[test]
    fn says_what_is_wrong_without_quoting_the_bundle() {
        let aes = include_bytes!("../tests/fixtures/ec.p12");
        assert_eq!(
            reason_for(aes, "wrong"),
            "the passphrase is wrong, or the bundle is damaged (its MAC does not match)"
        );
        assert!(
            reason_for(include_bytes!("../tests/fixtures/ec.rc4.p12"), "fixture")
                .starts_with("the bundle uses a cipher or MAC that is not supported")
        );
        assert_eq!(reason_for(b"fixture", "fixture"), "the file is not a valid PKCS#12 bundle");
    }

    #[test]
    fn wants_exactly_one_key_and_a_certificate() -> TestResult {
        let (cert, key) = parts()?;
        let chain =
            |certs: Vec<Certificate>| KeyStoreEntry::PrivateKeyChain(PrivateKeyChain::new("id", key.clone(), certs));
        assert_eq!(
            reason_for(&bundle(vec![KeyStoreEntry::Certificate(cert.clone())])?, "pw"),
            "the bundle has no private key"
        );
        assert_eq!(reason_for(&bundle(vec![chain(vec![])])?, "pw"), "the bundle has no certificate");
        let two = bundle(vec![chain(vec![cert.clone()]), chain(vec![cert])])?;
        assert_eq!(reason_for(&two, "pw"), "the bundle holds 2 private keys; export only the one to send");
        Ok(())
    }

    #[test]
    fn puts_the_key_owners_certificate_first() -> TestResult {
        let (cert, key) = parts()?;
        let ca = Certificate::from_der(&CertificateDer::from_pem_slice(include_bytes!("../tests/fixtures/ec.pem"))?)?;
        let entries = vec![
            KeyStoreEntry::Certificate(ca.clone()),
            KeyStoreEntry::PrivateKeyChain(PrivateKeyChain::new("id", key, [cert.clone()])),
        ];
        let decoded = decode(&bundle(entries)?, "pw")?;
        let ders: Vec<&[u8]> = decoded.chain.iter().map(AsRef::as_ref).collect();
        assert_eq!(ders, [cert.as_der(), ca.as_der()]);
        Ok(())
    }

    #[test]
    fn names_each_other_failure() {
        assert!(reason(&P12Error::UnpadError).contains("could not be decrypted"));
        assert!(reason(&P12Error::Pkcs5Error("x".into())).contains("could not be decrypted"));
        assert_eq!(reason(&P12Error::UnsupportedMacAlgorithm), reason(&P12Error::UnsupportedEncryptionScheme));
        assert!(reason(&P12Error::InvalidParameters).contains("iterations"));
        assert!(reason(&P12Error::InvalidPrivateKey).contains("PKCS#8"));
        assert!(reason(&P12Error::UnsupportedCertificateType).contains("X.509"));
        assert_eq!(reason(&P12Error::InvalidVersion), "the file is not a valid PKCS#12 bundle");
    }
}
