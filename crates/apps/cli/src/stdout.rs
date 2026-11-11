//! Standard output that its reader may stop reading. `vc call x | head -1` closes the pipe
//! after a line; what the reader did not read it did not want, so the command goes on quietly
//! and ends as it would have.

use std::io::{self, ErrorKind, StdoutLock, Write};

/// Standard output, locked; once its reader has gone, it takes every write without error.
pub fn out() -> Quiet<StdoutLock<'static>> {
    Quiet(io::stdout().lock())
}

/// A writer that drops what it is given once its reader has gone (a closed pipe).
pub struct Quiet<W>(W);

impl<W: Write> Write for Quiet<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        unread(self.0.write(buf), buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        unread(self.0.flush(), ())
    }
}

/// `result`, or `instead` when it failed only because no one reads any more.
fn unread<T>(result: io::Result<T>, instead: T) -> io::Result<T> {
    match result {
        Err(error) if error.kind() == ErrorKind::BrokenPipe => Ok(instead),
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, ErrorKind, Write};

    use super::Quiet;

    struct Gone(ErrorKind);

    impl Write for Gone {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(self.0.into())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(self.0.into())
        }
    }

    #[test]
    fn a_closed_pipe_takes_every_write_and_any_other_failure_stays() {
        let mut closed = Quiet(Gone(ErrorKind::BrokenPipe));
        assert!(writeln!(closed, "one line").and_then(|()| closed.flush()).is_ok());
        let mut full = Quiet(Gone(ErrorKind::StorageFull));
        assert_eq!(writeln!(full, "one line").map_err(|error| error.kind()), Err(ErrorKind::StorageFull));
    }
}
