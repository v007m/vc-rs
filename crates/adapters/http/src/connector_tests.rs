use std::net::{Ipv4Addr, SocketAddr};

use http::{HeaderValue, Uri};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::{Target, open, resolve, tunnel};
use crate::HttpAdapterError;

fn target(uri: &'static str) -> Target {
    Target::from_uri(&Uri::from_static(uri)).unwrap()
}

#[test]
fn targets_take_default_ports_by_scheme_and_unbracket_ipv6() {
    assert_eq!(target("http://h/x"), Target { host: "h".into(), port: 80, tls: false });
    assert_eq!(target("https://h/x"), Target { host: "h".into(), port: 443, tls: true });
    let v6 = target("https://[::1]:8443/");
    assert_eq!((v6.host.as_str(), v6.port), ("::1", 8443));
    assert_eq!(v6.authority(), "[::1]:8443");
    assert_eq!(target("http://h:81").authority(), "h:81");
}

#[test]
fn a_url_without_a_host_is_refused() {
    assert_eq!(Target::from_uri(&Uri::from_static("/relative")), Err(HttpAdapterError::NoHost("/relative".into())));
}

#[test]
fn an_https_proxy_is_refused() {
    let error = Target::proxy(&Uri::from_static("https://proxy:3128")).err();
    assert!(matches!(error, Some(HttpAdapterError::Connect { ref host, .. }) if host == "proxy:3128"), "{error:?}");
    assert!(Target::proxy(&Uri::from_static("socks5://proxy:1080")).is_err());
    assert!(Target::proxy(&Uri::from_static("http://proxy:3128")).is_ok());
}

#[tokio::test]
async fn an_ip_literal_needs_no_lookup_and_a_name_does() {
    let (addresses, dns) = resolve(&target("http://127.0.0.1:9/")).await.unwrap();
    assert_eq!((addresses, dns), (vec![SocketAddr::from((Ipv4Addr::LOCALHOST, 9))], None));
    let (addresses, dns) = resolve(&target("http://localhost:9/")).await.unwrap();
    assert!(!addresses.is_empty() && dns.is_some());
}

#[tokio::test]
async fn an_unknown_name_is_a_dns_error() {
    let error = resolve(&target("http://no-such-host.invalid/")).await.err();
    assert!(matches!(error, Some(HttpAdapterError::Dns { ref host, .. }) if host == "no-such-host.invalid"), "{error:?}");
}

#[tokio::test]
async fn a_closed_port_is_a_connect_error_and_an_open_one_is_timed() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    let open_target = Target { host: "127.0.0.1".into(), port: address.port(), tls: false };
    let (_stream, record) = open(&open_target).await.unwrap();
    assert_eq!((record.remote, record.dns, record.tls), (address, None, None));
    drop(listener);
    let error = open(&open_target).await.err();
    assert!(matches!(error, Some(HttpAdapterError::Connect { ref host, .. }) if *host == open_target.authority()));
}

/// A proxy that reads one CONNECT head, records it, and answers `reply`.
async fn proxy(reply: &'static [u8]) -> (SocketAddr, tokio::task::JoinHandle<String>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut head = vec![0_u8; 1024];
        let read = socket.read(&mut head).await.unwrap();
        socket.write_all(reply).await.unwrap();
        String::from_utf8_lossy(&head[..read]).into_owned()
    });
    (address, task)
}

#[tokio::test]
async fn tunnel_sends_connect_with_auth_and_accepts_a_200() {
    let (address, seen) = proxy(b"HTTP/1.1 200 Connection established\r\n\r\n").await;
    let mut stream = TcpStream::connect(address).await.unwrap();
    let auth = HeaderValue::from_static("Basic dTpw");
    tunnel(&mut stream, &target("https://api.test:8443/"), Some(&auth)).await.unwrap();
    let head = seen.await.unwrap();
    assert!(head.starts_with("CONNECT api.test:8443 HTTP/1.1\r\nHost: api.test:8443\r\n"), "{head}");
    assert!(head.contains("Proxy-Authorization: Basic dTpw\r\n") && head.ends_with("\r\n\r\n"), "{head}");
}

#[tokio::test]
async fn tunnel_reports_a_refusal_with_its_status_line() {
    let (address, _seen) = proxy(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n").await;
    let mut stream = TcpStream::connect(address).await.unwrap();
    let error = tunnel(&mut stream, &target("https://api.test/"), None).await.err();
    let expected = HttpAdapterError::Tunnel {
        host: "api.test:443".into(),
        reason: "HTTP/1.1 407 Proxy Authentication Required".into(),
    };
    assert_eq!(error, Some(expected));
}

#[tokio::test]
async fn tunnel_gives_up_on_a_proxy_that_closes_early() {
    let (address, _seen) = proxy(b"HTTP/1.1 200 OK\r\n").await;
    let mut stream = TcpStream::connect(address).await.unwrap();
    let error = tunnel(&mut stream, &target("https://api.test/"), None).await.err();
    assert!(matches!(error, Some(HttpAdapterError::Tunnel { .. })), "{error:?}");
}
