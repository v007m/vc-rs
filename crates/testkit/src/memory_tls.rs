use std::collections::BTreeMap;
use std::path::PathBuf;

use vc_interfaces::{ClientSource, LoadedTls, MaterialSource, TlsError, TlsLoader, TlsPart, TlsSpec};

/// A TLS loader over files held in memory, by path. It checks nothing but that each file is
/// there, and loads a [`MemoryTls`].
#[derive(Clone, Debug, Default)]
pub struct MemoryTlsLoader {
    files: BTreeMap<PathBuf, Vec<u8>>,
}

/// What a [`MemoryTlsLoader`] loads: each `ca` entry's bytes, and each identity's ID and hosts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct MemoryTls {
    /// The `ca` entries' bytes, in order.
    pub ca: Vec<Vec<u8>>,
    /// Each identity's ID and hosts, in order.
    pub clients: Vec<(String, Vec<String>)>,
}

impl MemoryTlsLoader {
    /// A loader holding `files`.
    #[must_use]
    pub fn new(files: &[(PathBuf, &[u8])]) -> Self {
        Self { files: files.iter().map(|(path, bytes)| (path.clone(), bytes.to_vec())).collect() }
    }

    /// The bytes of `source`, or `part`'s file refused as missing.
    fn read(&self, source: &MaterialSource, part: TlsPart) -> Result<Vec<u8>, TlsError> {
        match source {
            MaterialSource::Inline(bytes) => Ok(bytes.to_vec()),
            MaterialSource::File { path, shown } => self.files.get(path).cloned().ok_or_else(|| {
                let file = shown.as_ref().map_or_else(|| "its file".to_owned(), |shown| format!("`{shown}`"));
                TlsError::Unusable { part, reason: format!("cannot read {file}: not found") }
            }),
        }
    }
}

impl TlsLoader for MemoryTlsLoader {
    fn load(&self, spec: TlsSpec) -> Result<LoadedTls, TlsError> {
        let ca = spec.ca.iter().enumerate().map(|(index, source)| self.read(source, TlsPart::Ca(index)));
        let mut loaded = MemoryTls { ca: ca.collect::<Result<_, _>>()?, clients: Vec::new() };
        for client in spec.clients {
            let part = |field| TlsPart::Client { id: client.id.clone(), field: Some(field) };
            match &client.material {
                ClientSource::Pem { cert, key } => {
                    self.read(cert, part("cert"))?;
                    self.read(key, part("key"))?;
                }
                ClientSource::Pkcs12 { bundle, .. } => {
                    self.read(bundle, part("pkcs12"))?;
                }
            }
            loaded.clients.push((client.id, client.hosts));
        }
        Ok(LoadedTls::new(loaded))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use vc_interfaces::{ClientSource, ClientSpec, LoadedTls, MaterialSource, Passphrase, TlsLoader as _, TlsSpec};

    use super::{MemoryTls, MemoryTlsLoader};
    use crate::tls_loader_suite;

    #[test]
    fn passes_the_tls_loader_suite_and_loads_what_it_holds() {
        let path = PathBuf::from("/project/certs/bank.p12");
        let memory = MemoryTlsLoader::new(&[(path.clone(), b"bundle")]);
        tls_loader_suite(&memory);
        let bundle = MaterialSource::File { path, shown: None };
        let material = ClientSource::Pkcs12 { bundle, passphrase: Passphrase::default() };
        let client = ClientSpec { id: "bank".into(), hosts: vec!["a.test".into()], material };
        let root = MaterialSource::Inline(b"root".to_vec().into());
        let loaded = memory.load(TlsSpec { ca: vec![root], clients: vec![client] }).ok().and_then(LoadedTls::take);
        let want = MemoryTls { ca: vec![b"root".to_vec()], clients: vec![("bank".into(), vec!["a.test".into()])] };
        assert_eq!(loaded, Some(want));
    }
}
