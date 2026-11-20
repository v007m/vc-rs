//! Reading client certificate material: a PEM certificate or key, or a PKCS#12 bundle.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use zeroize::Zeroizing;

/// The most bytes a certificate, key or bundle file may hold: 1 MiB, far over any real one.
const LIMIT: usize = 1 << 20;

/// The bytes of the certificate, key or PKCS#12 bundle at `path`, in memory that is wiped when
/// dropped. The buffer is sized from the file's length, capped at the limit, plus one byte to
/// see the end, so a file that does not grow while it is read leaves no copy behind.
///
/// # Errors
///
/// The error opening or reading `path`, or [`io::ErrorKind::FileTooLarge`] when it holds more
/// than 1 MiB.
pub fn read_material(path: &Path) -> io::Result<Zeroizing<Vec<u8>>> {
    let file = File::open(path)?;
    let length = usize::try_from(file.metadata()?.len()).unwrap_or(LIMIT);
    let mut bytes = Zeroizing::new(Vec::with_capacity(length.min(LIMIT) + 1));
    file.take(LIMIT as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > LIMIT {
        return Err(io::Error::new(io::ErrorKind::FileTooLarge, "the file is over 1 MiB"));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::read_material;

    #[test]
    fn reads_a_file_up_to_1_mib_and_refuses_one_byte_more() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("key.pem");
        std::fs::write(&path, vec![b'k'; 1_048_576])?;
        assert_eq!(read_material(&path)?.len(), 1_048_576);
        std::fs::write(&path, vec![b'k'; 1_048_577])?;
        let error = read_material(&path).err().map(|error| (error.kind(), error.to_string()));
        assert_eq!(error, Some((io::ErrorKind::FileTooLarge, "the file is over 1 MiB".to_owned())));
        let gone = read_material(&dir.path().join("gone.pem")).err().map(|error| error.kind());
        assert_eq!(gone, Some(io::ErrorKind::NotFound));
        Ok(())
    }

    #[test]
    fn sizes_its_buffer_from_the_files_length_plus_one() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("cert.pem");
        std::fs::write(&path, vec![b'c'; 100])?;
        let bytes = read_material(&path)?;
        assert_eq!((bytes.len(), bytes.capacity()), (100, 101));
        Ok(())
    }
}
