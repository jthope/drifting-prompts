//! `ipcamera-server`: the cameras' library and their live video, for a browser.

use std::{
    io::{BufRead, Write},
    net::Ipv4Addr,
    path::{Path, PathBuf},
    process::ExitCode,
};

use anyhow::{Context, Result, anyhow, bail};
use ipcamera_core::{Library, config::ConfigStore};
use tokio::net::TcpListener;

/// Clear of nsh's development server (8790) and its web preview (8787).
const DEFAULT_PORT: u16 = 8770;

struct Options {
    settings: PathBuf,
    data: Option<PathBuf>,
    port: u16,
    serving: ipcamera_server::Options,
    password: Option<PasswordChange>,
}

enum PasswordChange {
    Set,
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

fn change_password(config: &ConfigStore, change: &PasswordChange) -> Result<()> {
    if let Some(problem) = config.problem() {
        bail!("{problem}");
    }
    let hash = match change {
        PasswordChange::Set => {
            // Typed here, never an argument, which the list of running programs would show.
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
