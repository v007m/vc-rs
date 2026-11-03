use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

use hyper::rt::{Read, ReadBufCursor, Write};
use hyper_util::client::legacy::connect::{Connected, Connection};
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;

/// How a connection was opened. Every response on the connection carries it (hyper copies the
/// connector's `extra` into each response); the first response to claim it reports the
/// connection as new, with these timings, and every later one as reused.
#[derive(Debug)]
pub struct ConnectRecord {
    /// Resolving the host; `None` for an IP literal.
    pub dns: Option<Duration>,
    /// Opening the TCP connection (and the proxy tunnel, when there is one).
    pub connect: Duration,
    /// The TLS handshake; `None` for plain HTTP.
    pub tls: Option<Duration>,
    /// The peer address.
    pub remote: SocketAddr,
    claimed: AtomicBool,
}

impl ConnectRecord {
    /// A record nobody has claimed yet.
    pub const fn new(dns: Option<Duration>, connect: Duration, remote: SocketAddr) -> Self {
        Self { dns, connect, tls: None, remote, claimed: AtomicBool::new(false) }
    }

    /// `true` exactly once: for the first response on the connection.
    pub fn claim(&self) -> bool {
        !self.claimed.swap(true, Ordering::AcqRel)
    }
}

/// A plain or TLS connection.
#[derive(Debug)]
pub enum Stream {
    /// Plain TCP.
    Plain(TcpStream),
    /// TLS over TCP.
    Tls(Box<TlsStream<TcpStream>>),
}

impl AsyncRead for Stream {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => Pin::new(stream).poll_read(cx, buf),
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Plain(stream) => Pin::new(stream).poll_write(cx, buf),
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => Pin::new(stream).poll_flush(cx),
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => Pin::new(stream).poll_shutdown(cx),
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_shutdown(cx),
        }
    }
}

/// What the connector hands hyper: the stream plus what hyper should know about it.
#[derive(Debug)]
pub struct Conn {
    io: TokioIo<Stream>,
    record: Arc<ConnectRecord>,
    proxied: bool,
    h2: bool,
}

impl Conn {
    /// Wraps `stream`. `proxied` makes hyper send absolute-form URLs (plain HTTP through a
    /// proxy); a TLS stream that negotiated `h2` makes it speak HTTP/2.
    pub fn new(stream: Stream, record: ConnectRecord, proxied: bool) -> Self {
        let h2 = matches!(&stream, Stream::Tls(tls) if crate::tls::negotiated_h2(tls));
        Self { io: TokioIo::new(stream), record: Arc::new(record), proxied, h2 }
    }
}

impl Connection for Conn {
    fn connected(&self) -> Connected {
        let connected = Connected::new().proxy(self.proxied).extra(Arc::clone(&self.record));
        if self.h2 { connected.negotiated_h2() } else { connected }
    }
}

impl Read for Conn {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: ReadBufCursor<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().io).poll_read(cx, buf)
    }
}

impl Write for Conn {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().io).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().io).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().io).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddr};
    use std::time::Duration;

    use super::ConnectRecord;

    #[test]
    fn a_record_is_claimed_exactly_once() {
        let record = ConnectRecord::new(None, Duration::from_millis(1), SocketAddr::from((Ipv4Addr::LOCALHOST, 80)));
        assert!(record.claim());
        assert!(!record.claim());
        assert!(!record.claim());
    }
}
