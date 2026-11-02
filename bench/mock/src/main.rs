//! The `vc-mock-server` binary: serves the mock API until Ctrl-C.

use std::net::{Ipv4Addr, SocketAddr};

use clap::Parser;
use vc_mock_server::{MockError, MockServer};

/// Mock HTTP server for Vibranium end-to-end tests and benchmarks.
#[derive(Parser)]
#[command(name = "vc-mock-server", version, about)]
struct Args {
    /// Port to listen on (127.0.0.1); 0 picks a free one.
    #[arg(long, default_value_t = 18080)]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<(), MockError> {
    let args = Args::parse();
    let handle = MockServer::start_on(SocketAddr::from((Ipv4Addr::LOCALHOST, args.port))).await?;
    println!("vc-mock-server listening on {}", handle.base_url());
    tokio::signal::ctrl_c().await?;
    handle.shutdown().await
}
