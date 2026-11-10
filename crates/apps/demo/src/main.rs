//! The `vc-demo` binary: the small users and echo service the `vc init` sample calls, on
//! localhost. `vc demo start` launches it (D175, D188); it serves until it is stopped.

mod routes;

use std::net::{Ipv4Addr, SocketAddr};

use clap::Parser;

/// The demo service of the `vc init` sample: `/users` and `/echo` on localhost.
#[derive(Parser)]
#[command(name = "vc-demo", version, about)]
struct Args {
    /// The port to listen on (127.0.0.1).
    #[arg(long, default_value_t = 7421)]
    port: u16,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();
    let listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, args.port))).await?;
    axum::serve(listener, routes::router()).await
}
