//! `ipcamera-server`: the cameras, their groups and views, and their live video, in a browser, with
//! no desktop app.
//!
//! ```text
//! ipcamera-server [--settings <file>] [--data <folder>] [--port <port>] [--app <folder>]
//!                [--allow-origin <origin>]
//! ipcamera-server --set-password | --clear-password [--settings <file>]
//! ```
//!
//! It reads `settings.json` beside the executable, the way the app does, unless `--settings`
//! names another: the desktop app's own works, since the files are the same. Its snapshots and
//! recordings go where that file's `storage` says, `data\snapshots` and `data\videos` beside it by
//! default, and its cameras are in `data\settings` beside it, unless `--data` names another folder
//! for them. So `--settings` at the desktop app's `settings.json` shares the app's cameras,
//! snapshots and videos. It serves the packaged web build of the app from `web` beside the
//! executable, unless `--app` names another folder. It listens on `http://127.0.0.1:8770/` unless
//! `--port` says otherwise, and only there. `--allow-origin` lets one more origin's pages use it,
//! such as `http://localhost:8787`, where `nsh run --target web` serves the app; give it once for
//! each. Ctrl+C stops it, once every moving camera has its stop and every recording's file is
//! finished.
//!
//! `--set-password` asks for a password on the terminal, saves its hash in `settings.json` and
//! ends: from its next start the server answers only those who sign in with it.
//! `--clear-password` takes it away again. The password is never an argument, which other
//! programs could read from the list of what is running.

use std::{
    io::{BufRead, Write},
    net::Ipv4Addr,
    path::{Path, PathBuf},
    process::ExitCode,
};

use anyhow::{Context, Result, anyhow, bail};
use ipcamera_core::{Library, config::ConfigStore};
use tokio::net::TcpListener;

/// The port the server listens on when `--port` doesn't say. nsh's development server takes 8790
/// (`nsh run`) and the web preview 8787, so this is clear of both.
const DEFAULT_PORT: u16 = 8770;

/// What the command line asks for.
struct Options {
    /// `settings.json`, which says where snapshots and recordings go.
    settings: PathBuf,
    /// The cameras, groups and views, when not `data\settings` beside `settings`.
    data: Option<PathBuf>,
    port: u16,
    serving: ipcamera_server::Options,
    /// A change to the server's password to make instead of serving.
    password: Option<PasswordChange>,
}

/// What to do with the server's password.
enum PasswordChange {
    /// Ask for one and save it.
    Set,
    /// Take it away.
    Clear,
}

impl Options {
    fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Self> {
        let mut options = Self {
            settings: beside_executable().join("settings.json"),
            data: None,
            port: DEFAULT_PORT,
            serving: ipcamera_server::Options {
                app: Some(beside_executable().join("web")),
                allow_origins: Vec::new(),
            },
            password: None,
        };
        while let Some(argument) = arguments.next() {
            let mut value = || {
                arguments
                    .next()
                    .ok_or_else(|| anyhow!("{argument} needs a value after it"))
            };
            match argument.as_str() {
                "--settings" => options.settings = PathBuf::from(value()?),
                "--data" => options.data = Some(PathBuf::from(value()?)),
                "--app" => options.serving.app = Some(PathBuf::from(value()?)),
                "--allow-origin" => {
                    let origin = value()?;
                    if !origin.starts_with("http://") && !origin.starts_with("https://") {
                        bail!("--allow-origin takes an origin such as http://127.0.0.1:8787");
                    }
                    options
                        .serving
                        .allow_origins
                        .push(origin.trim_end_matches('/').to_string());
                }
                "--set-password" => options.password = Some(PasswordChange::Set),
                "--clear-password" => options.password = Some(PasswordChange::Clear),
                "--port" => {
                    options.port = value()?
                        .parse()
                        .context("--port takes a number from 1 to 65535")?;
                }
                _ => bail!(
                    "unknown argument {argument}: use --settings <file>, --data <folder>, --port \
                     <port>, --app <folder>, --allow-origin <origin>, --set-password or \
                     --clear-password"
                ),
            }
        }
        Ok(options)
    }
}

/// The folder that holds this executable, where its data goes.
fn beside_executable() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ipcamera-server: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let options = Options::parse(std::env::args().skip(1))?;
    let config = ConfigStore::load(&options.settings);
    if let Some(change) = &options.password {
        return change_password(&config, change);
    }
    let data = options
        .data
        .clone()
        .unwrap_or_else(|| config.settings_dir());
    let library = Library::open(&data);
    let problems = config.problem().map(str::to_string).into_iter();
    for problem in problems.chain(library.problems()) {
        eprintln!("ipcamera-server: {problem}");
    }
    let (snapshots, videos) = (config.snapshots_dir(), config.videos_dir());

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, options.port))
            .await
            .with_context(|| format!("port {} is taken, or not allowed", options.port))?;
        println!(
            "IP Camera server on http://127.0.0.1:{}/ with the cameras in {}, snapshots in {} \
             and recordings in {}. {} Ctrl+C stops it.",
            options.port,
            data.display(),
            snapshots.display(),
            videos.display(),
            if config.current().server.password.is_empty() {
                "It has no password (--set-password gives it one)."
            } else {
                "It asks for its password."
            }
        );
        ipcamera_server::serve(listener, library, config, options.serving, async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
        Ok(())
    })
}

/// Sets or clears the server's password in `config`'s file, and says what it did.
fn change_password(config: &ConfigStore, change: &PasswordChange) -> Result<()> {
    if let Some(problem) = config.problem() {
        bail!("{problem}");
    }
    let hash = match change {
        PasswordChange::Set => {
            // Typed here rather than given as an argument, which the list of running programs
            // would show. The terminal shows it as it is typed.
            print!("The server's new password (it shows as you type): ");
            std::io::stdout().flush()?;
            let mut typed = String::new();
            std::io::stdin().lock().read_line(&mut typed)?;
            let password = typed.trim_end_matches(['\r', '\n']);
            ipcamera_server::hash_password(password).map_err(|why| anyhow!(why))?
        }
        PasswordChange::Clear => String::new(),
    };
    let set = !hash.is_empty();
    config
        .update(|config| config.server.password = hash)
        .with_context(|| format!("{} could not be written", config.path().display()))?;
    println!(
        "{} A server that is running keeps what it started with until it is started again.",
        if set {
            "The password is saved: the server asks for it from its next start."
        } else {
            "The password is gone: the server asks for none from its next start."
        }
    );
    Ok(())
}
