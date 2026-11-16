//! The repository's own commands, run through cargo. This is Rust's "xtask" convention: a small
//! program in the workspace that aliases in `.cargo/config.toml` run, so a command needs nothing
//! installed beyond what the build already needs, and reads the same in any shell. All of them are
//! run at the repository's root.
//!
//! Each product's commands go by its name. `cargo ipcamera` is IP Camera's desktop app: `nsh run`
//! in `crates/ipcamera/app`, a Win32 window with hot reload, its arguments after `cargo ipcamera`
//! (`--fps`, say). Ctrl+C stops it, and this waits until it has. `cargo mdviewer` is the same for
//! Markdown Viewer (`crates/mdviewer/app`), and `cargo package-mdviewer` its
//! `nsh package --target windows`, into its own `.nsh\out\mdviewer\`.
//!
//! `cargo package-ipcamera` and `cargo package-ipcamera-web` are `nsh package --target windows` and
//! `--target web` in `crates/ipcamera/app`, the arguments after them passed on (`--app-version 1.2.0`,
//! say). The Windows build goes where nsh puts it, `.nsh\out\ipcamera\` and `ipcamera.zip`; the web
//! build, a static site, goes to `.nsh\out\web\` unless `--out` says otherwise, apart from the
//! Windows build and ready to copy to `web` beside `ipcamera-server`. It is built without
//! `IPCAMERA_SERVER`, which `cargo ipcamera-web` and `.claude/launch.json` give a development build: the
//! packaged site is served by the server, and finds it at its own address. `cargo package` itself
//! is cargo's, which an alias can't take over.
//!
//! `cargo ipcamera-web` starts `ipcamera-server` and the web build's development server
//! (`nsh run --target web`) together, wired to each other as `.claude/launch.json`'s two entries
//! are: the server on port 8770, letting the page on 8787 use it, and the web build served from
//! 8787, built with `IPCAMERA_SERVER` naming the server. Both build and print to this console. The
//! server reads the desktop app's own `settings.json`, the one `nsh run` uses in
//! `crates\ipcamera\app\target\debug`, so the web build shows the desktop app's cameras, groups and
//! views, and its snapshots and recordings go to the same folders. The arguments after `cargo ipcamera-web`
//! go to the server, such as `--settings` for another settings file and `--data`
//! (`crates/ipcamera/server/src/main.rs`); only `--port` is refused, since the web build is told the
//! server's port when it builds.
//!
//! Ctrl+C reaches both, and this waits until both have ended, so the server finishes its
//! recordings first; a second Ctrl+C stops them without waiting. When one ends by itself (a build
//! error, or another `nsh run` taking over nsh's development session), this stops the other.
//!
//! `cargo screenshots` takes the READMEs' screenshots of both apps, light and dark, in real windows
//! on made-up cameras and projects, into `assets/screenshots/`; `cargo screenshots ipcamera` or
//! `mdviewer` takes one app's. The developer's own settings are set aside meanwhile and put back
//! ([`screenshots`]).
//!
//! What it starts never sees the variables `cargo run` set for this program (`CARGO_PKG_NAME`,
//! `CARGO_MANIFEST_DIR`, …): see [`without_cargo_run`].

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

/// IP Camera's folder in `crates/`: `cargo ipcamera`, `cargo ipcamera-web` and the packages.
const IPCAMERA: &str = "ipcamera";

/// Markdown Viewer's folder in `crates/`: `cargo mdviewer` and `cargo package-mdviewer`.
const MDVIEWER: &str = "mdviewer";

/// The server's port, which the web build is told when it builds: `ipcamera-server`'s default and
/// `.claude/launch.json`'s.
const SERVER_PORT: u16 = 8770;

/// The port `nsh run --target web` serves the page from, which the server is told to allow.
const WEB_PORT: u16 = 8787;

/// What a failure to start nsh says.
const NO_NSH: &str = "couldn't start nsh: README.md \"Setup\" installs it";

/// Where `cargo package-ipcamera-web` puts the site, from `crates/ipcamera/app`.
const WEB_OUT: &str = ".nsh/out/web";

fn main() -> ExitCode {
    let mut arguments: Vec<String> = std::env::args().skip(1).collect();
    let task = if arguments.is_empty() {
        String::new()
    } else {
        arguments.remove(0)
    };
    // Each task is the alias's name, as the person typed it, which the messages say.
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

/// `cargo ipcamera` and `cargo mdviewer`: `product`'s desktop app, until Ctrl+C or until nsh ends.
fn run(product: &str, arguments: Vec<String>) -> Result<ExitCode> {
    let mut app = nsh(&repository_root(), product);
    app.arg("run").args(&arguments);
    until_done(app)
}

/// `cargo package-mdviewer`: Markdown Viewer's Windows build, the arguments passed on.
fn package_mdviewer(arguments: Vec<String>) -> Result<ExitCode> {
    let mut build = nsh(&repository_root(), MDVIEWER);
    build
        .args(["package", "--target", "windows"])
        .args(&arguments);
    until_done(build)
}

/// `cargo package-ipcamera` and `cargo package-ipcamera-web`: IP Camera's `nsh package` for `target`
/// (`windows` or `web`), the arguments passed on.
fn package_ipcamera(target: &str, arguments: Vec<String>) -> Result<ExitCode> {
    let mut build = nsh(&repository_root(), IPCAMERA);
    build.args(["package", "--target", target]);
    if target == "web" {
        // The server serves the site, which finds it at its own address; a development build's
        // address would send it elsewhere.
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

/// Runs `command`, an nsh, until it ends, and ends as it does. Ctrl+C reaches nsh too, and this
/// waits until nsh has stopped; a second Ctrl+C stops it at once.
fn until_done(mut command: Command) -> Result<ExitCode> {
    runtime()?.block_on(async {
        // `kill_on_drop` stops it on a second Ctrl+C.
        let mut nsh = command.spawn().context(NO_NSH)?;
        tokio::select! {
            // As `web`'s: a Ctrl+C that also ends nsh in the same instant is the Ctrl+C.
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

/// `cargo ipcamera-web`: the server and the web build together, until Ctrl+C or until one of them ends.
fn web(server_arguments: Vec<String>) -> Result<ExitCode> {
    if server_arguments.iter().any(|argument| argument == "--port") {
        bail!(
            "the server stays on port {SERVER_PORT}, where the web build looks for it; run \
             `cargo run -p ipcamera-server` by itself for another port"
        );
    }
    let root = repository_root();
    runtime()?.block_on(async {
        // `kill_on_drop` stops both if anything below fails, and on a second Ctrl+C. Stopping
        // cargo stops the server too: cargo keeps what it runs in a job that ends with it.
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
            // Ctrl+C first, so one that also ends a program in the same instant is taken as the
            // Ctrl+C rather than as that program ending by itself.
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

/// One of the two ended by itself: says so, and stops the other, which is no use alone.
async fn ended(name: &str, status: ExitStatus, other_name: &str, other: &mut Child) -> ExitCode {
    eprintln!("cargo ipcamera-web: {name} ended ({status}), so {other_name} is stopping too");
    // It may have just ended as well, and `kill_on_drop` tries again.
    let _ = other.kill().await;
    exit_code(status)
}

/// Success when what it ran succeeded.
fn exit_code(status: ExitStatus) -> ExitCode {
    if status.success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// A runtime to wait on the programs and Ctrl+C at once. Its Ctrl+C handler keeps this running
/// while they end, as they get the Ctrl+C themselves.
fn runtime() -> Result<Runtime> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?)
}

/// `nsh` in `product`'s app folder (`crates/<product>/app`), stopped if this ends first.
fn nsh(root: &Path, product: &str) -> Command {
    let mut nsh = Command::new("nsh");
    without_cargo_run(&mut nsh);
    nsh.current_dir(root.join("crates").join(product).join("app"))
        .kill_on_drop(true);
    nsh
}

/// Keeps the variables `cargo run` set for this program from `command`, which builds with cargo.
///
/// A build script that reads one asks cargo to build its crate again when it changes, and cargo
/// compares the value in its own environment. `ring`'s reads `CARGO_PKG_NAME` and
/// `CARGO_MANIFEST_DIR`, and everything that uses HTTPS stands on `ring`: passed on, xtask's values
/// made each `cargo ipcamera` after a plain `nsh run`, an `nsh test` or a `cargo test` build
/// `rustls`, nsh's crates and the app again (about 100 seconds), and the next of those undo it.
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

/// The repository's root, two folders above this crate's.
fn repository_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(2)
        .unwrap_or(manifest)
        .to_path_buf()
}

/// The `settings.json` that `nsh run` reads and writes, beside the desktop app's debug build: the
/// server takes its cameras, groups and views from `data\settings` beside it, so the web build
/// shows what the desktop app does, and its snapshots and recordings go where the app's go.
fn desktop_settings(root: &Path) -> PathBuf {
    root.join("crates")
        .join("ipcamera")
        .join("app")
        .join("target")
        .join("debug")
        .join("settings.json")
}

/// The cargo that runs this, which `cargo run` names in `CARGO`, so the server builds with the
/// same toolchain.
fn cargo() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}
