use p12_keystore::{Certificate, PrivateKey};
use rcgen::string::PrintableString;
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, DnValue, IsCa, Issuer, KeyPair, date_time_ymd,
};

use super::tests::{cert_bag, chain_for};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// A certificate authority for these tests: its parameters and key.
struct Ca {
    params: CertificateParams,
    key: KeyPair,
}

impl Ca {
    /// A CA named `name` (a common name), expiring at the start of `expires`.
    fn new(name: DnValue, expires: i32) -> TestResult<Self> {
        let mut params = CertificateParams::new(Vec::<String>::new())?;
        params.distinguished_name = DistinguishedName::new();
        params.distinguished_name.push(DnType::CommonName, name);
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.not_after = date_time_ymd(expires, 1, 1);
        Ok(Self { params, key: KeyPair::generate()? })
    }

    /// The same CA with another key.
    fn with_key(self, key: KeyPair) -> Self {
        Self { key, ..self }
    }

    fn root(&self) -> TestResult<Certificate> {
        Ok(Certificate::from_der(self.params.self_signed(&self.key)?.der())?)
    }

    /// This CA's certificate, signed by `by`.
    fn signed_by(&self, by: &Self) -> TestResult<Certificate> {
        let cert = self.params.signed_by(&self.key, &Issuer::from_params(&by.params, &by.key))?;
        Ok(Certificate::from_der(cert.der())?)
    }

    /// A leaf certificate and its key, signed by this CA.
    fn leaf(&self) -> TestResult<(Certificate, PrivateKey)> {
        let leaf = Self { params: CertificateParams::new(vec!["client.test".to_owned()])?, key: KeyPair::generate()? };
        Ok((leaf.signed_by(self)?, PrivateKey::from_der(leaf.key.serialized_der())?))
    }
}

fn copy(key: &KeyPair) -> TestResult<KeyPair> {
    Ok(KeyPair::try_from(key.serialized_der())?)
}

#[test]
fn walks_through_an_intermediate_stored_as_a_trusted_bag() -> TestResult {
    let (root, intermediate) = (Ca::new("root".into(), 2090)?, Ca::new("intermediate".into(), 2080)?);
    let (root_cert, intermediate_cert) = (root.root()?, intermediate.signed_by(&root)?);
    let (leaf, key) = intermediate.leaf()?;
    let certs =
        vec![cert_bag(&root_cert, None, true), cert_bag(&leaf, None, false), cert_bag(&intermediate_cert, None, true)];
    assert_eq!(chain_for(key, None, certs)?, [leaf.as_der(), intermediate_cert.as_der(), root_cert.as_der()]);
    Ok(())
}

#[test]
fn picks_the_renewed_intermediate_that_signed_the_child_then_the_one_that_expires_last() -> TestResult {
    let root = Ca::new("root".into(), 2090)?;
    let old = Ca::new("intermediate".into(), 2060)?;
    let renewed = Ca::new("intermediate".into(), 2080)?.with_key(copy(&old.key)?);
    let rekeyed = Ca::new("intermediate".into(), 2085)?;
    let (root_cert, old_cert) = (root.root()?, old.signed_by(&root)?);
    let (renewed_cert, rekeyed_cert) = (renewed.signed_by(&root)?, rekeyed.signed_by(&root)?);
    let (leaf, key) = old.leaf()?;
    let bags = |intermediates: [&Certificate; 2]| {
        let mut bags = vec![cert_bag(&leaf, None, false)];
        bags.extend(intermediates.map(|cert| cert_bag(cert, None, false)));
        bags.push(cert_bag(&root_cert, None, false));
        bags
    };
    let to = |intermediate: &Certificate| {
        vec![leaf.as_der().to_vec(), intermediate.as_der().to_vec(), root_cert.as_der().to_vec()]
    };
    assert_eq!(chain_for(key.clone(), None, bags([&rekeyed_cert, &old_cert]))?, to(&old_cert));
    assert_eq!(chain_for(key.clone(), None, bags([&old_cert, &rekeyed_cert]))?, to(&old_cert));
    assert_eq!(chain_for(key.clone(), None, bags([&old_cert, &renewed_cert]))?, to(&renewed_cert));
    assert_eq!(chain_for(key, None, bags([&renewed_cert, &old_cert]))?, to(&renewed_cert));
    Ok(())
}

#[test]
fn matches_names_by_their_der_not_by_how_they_render() -> TestResult {
    let printable = DnValue::PrintableString(PrintableString::try_from("shared name")?);
    let root = Ca::new(printable, 2090)?;
    let intermediate = Ca::new(DnValue::Utf8String("shared name".to_owned()), 2080)?;
    let (root_cert, intermediate_cert) = (root.root()?, intermediate.signed_by(&root)?);
    assert_eq!(root_cert.subject(), intermediate_cert.subject(), "both render as {}", root_cert.subject());
    assert_eq!(intermediate_cert.issuer(), intermediate_cert.subject(), "and look self-issued");
    let (leaf, key) = intermediate.leaf()?;
    let bag = |cert| cert_bag(cert, None, false);
    let all = vec![bag(&leaf), bag(&root_cert), bag(&intermediate_cert)];
    assert_eq!(chain_for(key.clone(), None, all)?, [leaf.as_der(), intermediate_cert.as_der(), root_cert.as_der()]);
    assert_eq!(chain_for(key, None, vec![bag(&leaf), bag(&root_cert)])?, [leaf.as_der()]);
    Ok(())
}
