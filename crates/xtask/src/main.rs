//! The repository's own commands: running and packaging each product's app, and the screenshots.
//! A first Ctrl+C lets what it started end on its own; a second stops it at once.

mod screenshots;

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{ExitCode, ExitStatus},
};

use anyhow::{Context, Result, anyhow, bail};
use tokio::{
    process::{Child, Command},
    runtime::Runtime,
};

const IPCAMERA: &str = "ipcamera";

const MDVIEWER: &str = "mdviewer";

/// Must match `ipcamera-server`'s default and `.claude/launch.json`'s.
const SERVER_PORT: u16 = 8770;

/// Must match `.claude/launch.json`'s.
const WEB_PORT: u16 = 8787;

const NO_NSH: &str = "couldn't start nsh: README.md \"Setup\" installs it";

const WEB_OUT: &str = ".nsh/out/web";

fn main() -> ExitCode {
    let mut arguments: Vec<String> = std::env::args().skip(1).collect();
    let task = if arguments.is_empty() {
        String::new()
    } else {
        arguments.remove(0)
    };
    let result = match task.as_str() {
        "ipcamera" => run(IPCAMERA, arguments),
        "ipcamera-web" => web(arguments),
        "package-ipcamera" => package_ipcamera("windows", arguments),
        "package-ipcamera-web" => package_ipcamera("web", arguments),
        "mdviewer" => run(MDVIEWER, arguments),
        "package-mdviewer" => package_mdviewer(arguments),
        "screenshots" => screenshots::take(arguments),
        _ => Err(anyhow!(
            "the tasks are `ipcamera`, `ipcamera-web`, `package-ipcamera`, `package-ipcamera-web`, \
             `mdviewer`, `package-mdviewer` and `screenshots`: run each as `cargo <task>` at the \
             repository's root"
        )),
    };
    result.unwrap_or_else(|error| {
        eprintln!("cargo {task}: {error:#}");
        ExitCode::FAILURE
    })
}

fn run(product: &str, arguments: Vec<String>) -> Result<ExitCode> {
    let mut app = nsh(&repository_root(), product);
    app.arg("run").args(&arguments);
    until_done(app)
}

fn package_mdviewer(arguments: Vec<String>) -> Result<ExitCode> {
    let mut build = nsh(&repository_root(), MDVIEWER);
    build
        .args(["package", "--target", "windows"])
        .args(&arguments);
    until_done(build)
}

fn package_ipcamera(target: &str, arguments: Vec<String>) -> Result<ExitCode> {
    let mut build = nsh(&repository_root(), IPCAMERA);
    build.args(["package", "--target", target]);
    if target == "web" {
        // The packaged site finds the server at its own address.
        build.env_remove("IPCAMERA_SERVER");
        let out_given = arguments.iter().any(|argument| {
            argument == "-o" || argument == "--out" || argument.starts_with("--out=")
        });
        if !out_given {
            build.args(["--out", WEB_OUT]);
        }
    }
    build.args(&arguments);
    until_done(build)
}

fn until_done(mut command: Command) -> Result<ExitCode> {
    runtime()?.block_on(async {
        // `kill_on_drop` stops it on a second Ctrl+C.
        let mut nsh = command.spawn().context(NO_NSH)?;
        tokio::select! {
            // A Ctrl+C that ends nsh in the same instant counts as the Ctrl+C.
            biased;
            signal = tokio::signal::ctrl_c() => {
                signal?;
                // nsh got the Ctrl+C too, and is ending.
                tokio::select! {
                    _ = nsh.wait() => {}
                    _ = tokio::signal::ctrl_c() => {}
                }
                Ok(ExitCode::SUCCESS)
            }
            status = nsh.wait() => Ok(exit_code(status?)),
        }
    })
}

fn web(server_arguments: Vec<String>) -> Result<ExitCode> {
    if server_arguments.iter().any(|argument| argument == "--port") {
        bail!(
            "the server stays on port {SERVER_PORT}, where the web build looks for it; run \
             `cargo run -p ipcamera-server` by itself for another port"
        );
    }
    let root = repository_root();
    runtime()?.block_on(async {
        // `kill_on_drop` stops both on a failure below or a second Ctrl+C. Stopping cargo stops the
        // server: cargo keeps what it runs in a job that ends with it.
        let mut server = Command::new(cargo());
        without_cargo_run(&mut server);
        server
            .current_dir(&root)
            .args(["run", "-p", "ipcamera-server", "--", "--port"])
            .arg(SERVER_PORT.to_string())
            .args(["--allow-origin", &format!("http://localhost:{WEB_PORT}")])
            .args(["--allow-origin", &format!("http://127.0.0.1:{WEB_PORT}")]);
        if !server_arguments.iter().any(|argument| argument == "--settings") {
            server.arg("--settings").arg(desktop_settings(&root));
        }
        let mut server = server
            .args(&server_arguments)
            .kill_on_drop(true)
            .spawn()
            .context("couldn't start cargo for the server")?;
        let mut web = nsh(&root, IPCAMERA)
            .args(["run", "--target", "web", "--port"])
            .arg(WEB_PORT.to_string())
            .env("IPCAMERA_SERVER", format!("http://127.0.0.1:{SERVER_PORT}"))
            .spawn()
            .context(NO_NSH)?;

        tokio::select! {
            // Ctrl+C first, so one that ends a program in the same instant counts as the Ctrl+C.
            biased;
            signal = tokio::signal::ctrl_c() => {
                signal?;
                // Both got the Ctrl+C too, and are ending.
                tokio::select! {
                    _ = async {
                        let _ = server.wait().await;
                        let _ = web.wait().await;
                    } => {}
                    _ = tokio::signal::ctrl_c() => {}
                }
                Ok(ExitCode::SUCCESS)
            }
            status = server.wait() => Ok(ended("the server", status?, "the web build", &mut web).await),
            status = web.wait() => Ok(ended("the web build", status?, "the server", &mut server).await),
        }
    })
}

async fn ended(name: &str, status: ExitStatus, other_name: &str, other: &mut Child) -> ExitCode {
    eprintln!("cargo ipcamera-web: {name} ended ({status}), so {other_name} is stopping too");
    // It may have ended already; `kill_on_drop` tries again.
    let _ = other.kill().await;
    exit_code(status)
}

fn exit_code(status: ExitStatus) -> ExitCode {
    if status.success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Its Ctrl+C handler keeps this running while the programs end on their own Ctrl+C.
fn runtime() -> Result<Runtime> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?)
}

fn nsh(root: &Path, product: &str) -> Command {
    let mut nsh = Command::new("nsh");
    without_cargo_run(&mut nsh);
    nsh.current_dir(root.join("crates").join(product).join("app"))
        .kill_on_drop(true);
    nsh
}

/// Keeps the variables `cargo run` set for this program from `command`: `ring`'s build script
/// watches `CARGO_PKG_NAME` and `CARGO_MANIFEST_DIR`, and passed on they rebuild nsh and the app.
fn without_cargo_run(command: &mut Command) {
    for (name, _) in std::env::vars_os() {
        if let Some(name) = name.to_str()
            && (name.starts_with("CARGO_PKG_")
                || matches!(
                    name,
                    "CARGO_MANIFEST_DIR"
                        | "CARGO_MANIFEST_PATH"
                        | "CARGO_CRATE_NAME"
                        | "CARGO_BIN_NAME"
                        | "CARGO_PRIMARY_PACKAGE"
                ))
        {
            command.env_remove(name);
        }
    }
}

fn repository_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(2)
        .unwrap_or(manifest)
        .to_path_buf()
}

/// The `settings.json` the desktop app's `nsh run` uses, in `dp_ui::development_folder`.
fn desktop_settings(root: &Path) -> PathBuf {
    root.join("crates")
        .join("ipcamera")
        .join("app")
        .join("target")
        .join("debug")
        .join(".nsh")
        .join("settings.json")
}

/// `cargo run`'s own cargo (`CARGO`), so the server builds with the same toolchain.
fn cargo() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}
