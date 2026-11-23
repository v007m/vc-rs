use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;
use vc_interfaces::{
    ClientSource, Environment, MaterialSource, TlsError, TlsPart, TlsSpec, Transport, TransportFactory, TransportPorts,
};

/// A transport factory over files held in memory, by path. It checks nothing but that each file
/// is there, sends what it read as a [`MemoryTls`], and gives its one transport.
#[derive(Clone)]
pub struct MemoryTransports {
    files: BTreeMap<PathBuf, Vec<u8>>,
    transport: Arc<dyn Transport>,
    read: mpsc::UnboundedSender<MemoryTls>,
}

/// What a [`MemoryTransports`] read for one transport: each `ca` entry's bytes, and each
/// identity's ID and hosts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct MemoryTls {
    /// The `ca` entries' bytes, in order.
    pub ca: Vec<Vec<u8>>,
    /// Each identity's ID and hosts, in order.
    pub clients: Vec<(String, Vec<String>)>,
}

impl fmt::Debug for MemoryTransports {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryTransports").field("files", &self.files.keys()).finish_non_exhaustive()
    }
}

impl MemoryTransports {
    /// A factory holding `files` and giving `transport`, and the receiving end of what each build
    /// read.
    #[must_use]
    pub fn new(
        files: &[(PathBuf, &[u8])],
        transport: Arc<dyn Transport>,
    ) -> (Self, mpsc::UnboundedReceiver<MemoryTls>) {
        let (read, built) = mpsc::unbounded_channel();
        let files = files.iter().map(|(path, bytes)| (path.clone(), bytes.to_vec())).collect();
        (Self { files, transport, read }, built)
    }

    /// The bytes of `source`, or `part`'s file refused as missing.
    fn read(&self, source: &MaterialSource, part: TlsPart) -> Result<Vec<u8>, TlsError> {
        match source {
            MaterialSource::Inline(text) => Ok(text.expose().as_bytes().to_vec()),
            MaterialSource::File { path, shown } => self.files.get(path).cloned().ok_or_else(|| {
                let file = shown.as_ref().map_or_else(|| "its file".to_owned(), |shown| format!("`{shown}`"));
                TlsError::Unusable { part, reason: format!("cannot read {file}: not found") }
            }),
        }
    }
}

impl TransportFactory for MemoryTransports {
    fn build(&self, tls: &TlsSpec, _: &dyn Environment) -> Result<TransportPorts, TlsError> {
        let ca = tls.ca.iter().enumerate().map(|(index, source)| self.read(source, TlsPart::Ca(index)));
        let mut read = MemoryTls { ca: ca.collect::<Result<_, _>>()?, clients: Vec::new() };
        for client in &tls.clients {
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
            read.clients.push((client.id.clone(), client.hosts.clone()));
        }
        self.read.send(read).ok();
        Ok(TransportPorts::from(Arc::clone(&self.transport)))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use vc_interfaces::{ClientSource, ClientSpec, MaterialSource, Secret, TlsSpec, TransportFactory as _};

    use super::{MemoryTls, MemoryTransports};
    use crate::{FakeTransport, MemoryEnvironment, status, transport_factory_suite};

    #[test]
    fn passes_the_transport_factory_suite_and_reads_what_it_holds() {
        let path = PathBuf::from("/project/certs/bank.p12");
        let transport = Arc::new(FakeTransport::new(|_, _| Ok(status(200))));
        let (memory, mut built) = MemoryTransports::new(&[(path.clone(), b"bundle")], transport);
        transport_factory_suite(&memory);
        while built.try_recv().is_ok() {}
        let bundle = MaterialSource::File { path, shown: None };
        let material = ClientSource::Pkcs12 { bundle, passphrase: Secret::new("") };
        let client = ClientSpec { id: "bank".into(), hosts: vec!["a.test".into()], material };
        let root = MaterialSource::Inline(Secret::new("root"));
        let spec = TlsSpec { ca: vec![root], clients: vec![client] };
        assert!(memory.build(&spec, &MemoryEnvironment::new(&[])).is_ok());
        let want = MemoryTls { ca: vec![b"root".to_vec()], clients: vec![("bank".into(), vec!["a.test".into()])] };
        assert_eq!(built.try_recv().ok(), Some(want));
    }
}
