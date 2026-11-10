//! `vc ui`: the local web UI, its engine service at `/api` and MCP at `/mcp`, over one engine,
//! on 127.0.0.1 only (G11). The only place the access token is shown is the one
//! `vc ui: <url>` line on stdout, printed whether or not a browser is opened. A build without the `web-ui` feature refuses with exit 2 (G12).

use std::path::Path;

use clap::Args;

use crate::Outcome;
use crate::explain::LayerArgs;

/// `vc ui`.
#[derive(Args)]
#[cfg_attr(not(feature = "web-ui"), expect(dead_code, reason = "only the web-ui build reads the flags"))]
pub struct UiArgs {
    /// The port on 127.0.0.1 to listen on.
    #[arg(long, default_value_t = 7420)]
    port: u16,
    /// Prints the sign-in URL without opening a browser.
    #[arg(long)]
    no_open: bool,
    #[command(flatten)]
    layers: LayerArgs,
}

/// `vc ui` in a build without the web UI: exit 2.
#[cfg(not(feature = "web-ui"))]
#[expect(clippy::unnecessary_wraps, reason = "the same signature as the web-ui build's `run`")]
pub fn run(_args: &UiArgs, _project: Option<&Path>) -> anyhow::Result<Outcome> {
    let help = "build vc with `pnpm -C web build`, then `cargo build -p vc-cli --features web-ui`";
    Ok(Outcome::Helped("this vc was built without UI".to_owned(), Some(help.to_owned())))
}

/// `vc ui [--port] [--no-open] [--profile] [--set]`: serves until Ctrl-C, then exit 0; exit 2
/// when the project cannot be found or the port cannot be listened on.
#[cfg(feature = "web-ui")]
pub fn run(args: &UiArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match crate::project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let layers = args.layers.request();
    let mcp = vc_mcp::VcServer::new(located.engine, layers.profiles, layers.sets);
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(serve::serve(mcp, args))
}

#[cfg(feature = "web-ui")]
mod serve {
    use std::process::{Command, Stdio};
    use std::sync::Arc;

    use vc_mcp::VcServer;
    use vc_server::{EmbeddedUi, EngineApi, LaunchUrl, ServerBuilder, ServerError};

    use super::UiArgs;
    use crate::Outcome;

    /// The program that opens a URL in the default browser, and its leading arguments.
    #[cfg(target_os = "macos")]
    const OPENER: &[&str] = &["open"];
    #[cfg(target_os = "windows")]
    const OPENER: &[&str] = &["cmd", "/C", "start", ""];
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    const OPENER: &[&str] = &["xdg-open"];

    /// Binds, prints the launch URL, opens it unless `--no-open`, and serves until Ctrl-C.
    pub async fn serve(mcp: VcServer, args: &UiArgs) -> anyhow::Result<Outcome> {
        let layers = args.layers.request();
        let engine = EngineApi::new(mcp.engine(), layers.profiles, layers.sets).into_service();
        let (service, stop_mcp) = mcp.into_http();
        let ui = EmbeddedUi::default();
        if ui.is_empty() {
            eprintln!("warning: this vc was built before `web/build` existed, so only /api and /mcp answer");
        }
        let builder = ServerBuilder::new(args.port).service(engine).mount("/mcp", service).files(Arc::new(ui));
        let server = match builder.bind().await {
            Ok(server) => server,
            Err(error @ ServerError::Bind { .. }) => return Ok(not_listening(&error)),
            Err(error) => return Err(error.into()),
        };
        println!("vc ui: {}", server.launch_url().expose());
        if !args.no_open {
            open(server.launch_url());
        }
        let stop = async move {
            let _ = tokio::signal::ctrl_c().await;
            stop_mcp();
        };
        server.serve(stop).await?;
        Ok(Outcome::Done)
    }

    fn not_listening(error: &ServerError) -> Outcome {
        let help = "pass another --port, or stop the program that is using this one";
        Outcome::Helped(error.to_string(), Some(help.to_owned()))
    }

    /// Opens `url` in the default browser on a thread of its own, which waits for the opener.
    /// The URL is on the opener's command line only while it runs.
    fn open(url: &LaunchUrl) {
        let Some((program, leading)) = OPENER.split_first() else { return };
        let mut command = Command::new(program);
        command.args(leading).arg(url.expose()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        std::thread::spawn(move || {
            if !command.status().is_ok_and(|status| status.success()) {
                eprintln!("vc ui: could not open a browser; open the URL above");
            }
        });
    }
}
