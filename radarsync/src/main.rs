mod db;
mod progress;

use std::{
    fmt,
    io::IsTerminal,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
    time::Duration,
};

use anyhow::{bail, Context};
use clap::{Parser, ValueEnum};
use db::Library;
use doppler_ws::device::DeviceClient;
use mime_guess::Mime;
use progress::Progression;
use tokio::sync::{mpsc, OwnedSemaphorePermit, Semaphore};
use tracing::level_filters::LevelFilter;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum ProgressMode {
    /// Always show a progress bar.
    On,
    /// Never show a progress bar.
    Off,
    /// Show a progress bar if the output is shown on the terminal, and -q is
    /// not defined.
    #[default]
    Auto,
}

impl fmt::Display for ProgressMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::On => "on",
            Self::Off => "off",
            Self::Auto => "auto",
        }
        .fmt(f)
    }
}

/// Utility to transfer music to Doppler for iOS
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Make the output noisier
    ///
    /// Set -v multiple times to make the output even more verbose. For example,
    /// -vvv will show just about everything.
    #[arg(short, action = clap::ArgAction::Count, conflicts_with = "quiet")]
    verbose: u8,
    /// Disable all output
    ///
    /// If --device isn't used, this will still print the pairing prompt.
    #[arg(short, long)]
    quiet: bool,
    /// Sync all music files recursively
    #[arg(short, long)]
    recurse: bool,
    /// How to display upload progress
    #[arg(long, default_value_t)]
    progress: ProgressMode,
    /// Number of upload tasks to run simultaneously
    #[arg(short, long, default_value_t = 5)]
    tasks: u8,
    /// Sync to a saved device
    #[arg(short, long)]
    device: Option<String>,
    /// List all saved devices
    #[arg(long, conflicts_with = "paths")]
    list_devices: bool,
    /// Forget the named device
    #[arg(long, conflicts_with = "paths")]
    drop_device: Option<String>,
    /// Disable the QR Code display
    #[arg(long)]
    no_qr: bool,
    /// Paths to transfer to the device
    #[arg(required = true)]
    paths: Vec<PathBuf>,
}

fn init_args() -> Args {
    let mut args = Args::parse();

    // The progress bar should be shown with 'auto' if:
    // - stdout is a tty
    // - quiet is not set

    if std::io::stderr().is_terminal() && !args.quiet {
        args.progress = ProgressMode::On;
    } else {
        args.progress = ProgressMode::Off;
    }

    // Set the log level according to the arguments
    let log_level = if args.quiet {
        // No messages
        LevelFilter::OFF
    } else {
        match args.verbose {
            0 => LevelFilter::WARN,
            1 => LevelFilter::INFO,
            2 => LevelFilter::DEBUG,
            3.. => LevelFilter::TRACE,
        }
    };

    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(false)
        .with_max_level(log_level)
        .init();

    args
}

// Wrapper for app_main
fn main() -> ExitCode {
    let args = init_args();

    if let Err(err) = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async move { app_main(args).await })
    {
        tracing::error!("{err}");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

async fn process_file<'a, P: AsRef<Path>>(
    device: &DeviceClient,
    mime: Mime,
    path: &'a P,
    _permit: OwnedSemaphorePermit,
) -> anyhow::Result<()> {
    tracing::info!("Uploading {}", path.as_ref().display());
    let file = tokio::fs::File::open(path).await?;

    let meta = file.metadata().await?;
    device.upload(path, meta.len(), mime, file).await?;

    Ok(())
}

async fn process_all_paths(
    device: Arc<DeviceClient>,
    selected: Vec<(PathBuf, Mime)>,
    sender: mpsc::Sender<anyhow::Error>,
    max_tasks: usize,
    progress: Progression,
) {
    let semaphore = Arc::new(Semaphore::new(max_tasks));

    let mut tasks = Vec::new();
    for (path, mime) in selected {
        let progress = progress.clone();
        let sender = sender.clone();
        let device = device.clone();
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let task = tokio::spawn(async move {
            if let Err(err) = process_file(&device, mime, &path, permit)
                .await
                .with_context(|| format!("{}", path.display()))
            {
                //
                let str_err = err.to_string();
                if sender.send(err).await.is_err() {
                    tracing::error!("I have no receiver and I must scream: {str_err}");
                }
            }
            progress.inc(1);
        });
        tasks.push(task);
    }
}

/// Recursively get all file paths in a directory.
fn get_dir_paths(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    tracing::trace!("reading dir {}", dir.display());
    let mut paths = Vec::new();
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry.with_context(|| format!("while recursing {}", dir.display()))?;
            let path = entry.path();
            if path.is_dir() {
                paths.append(&mut get_dir_paths(&path)?);
            } else {
                paths.push(path);
            }
        }
    }

    Ok(paths)
}

async fn app_main(args: Args) -> anyhow::Result<()> {
    let mut api = doppler_ws::TransferClient::connect()
        .await
        .context("Error accessing Doppler API")?;
    let library = Library::open().await?;

    // First, process the short-circuit stuff
    if args.list_devices {
        let names = library.device_names().await?;
        println!("Saved devices:");
        for name in names {
            println!("  {name}");
        }
        std::process::exit(0);
    } else if let Some(name) = args.drop_device {
        library.delete_device(&name).await?;
        println!("Device {name} forgotten.");
        std::process::exit(0);
    }

    let mut response = if let Some(device) = args.device {
        // Perform the saved device pairing flow
        let Some(device) = library.get_device(&device).await? else {
            bail!("Device name not found");
        };
        let spin = Progression::new_spinner(
            args.progress,
            format!(
                "Waiting for {} to respond...",
                device.name.as_deref().unwrap_or("device")
            ),
        );
        spin.enable_steady_tick(Duration::from_millis(300));
        let result = api.get_saved_device(&device).await;
        spin.finish_and_clear();
        result
    } else {
        // Pair by code
        let pairing_code = api.code();
        if !args.no_qr {
            let qrcode =
                qrencode::QrCode::new(pairing_code).context("Failed to generate QR code")?;
            let encoded = qrcode.render::<char>().module_dimensions(2, 1).build();
            println!("{encoded}");
        }

        println!("Use code {pairing_code} to connect your device.");

        api.get_new_device().await
    }
    .context("Failed to pair")?;

    // Check if we've previously saved the device
    let is_saved = matches!(library.get_device_by_id(response.id()).await, Ok(Some(_)));

    let device = api
        .confirm_device(&mut response, is_saved)
        .await
        .context("Couldn't get device URL")?;

    // If the device reports a push token, that means the device requested to be saved
    if let Some(push_token) = device.push_token() {
        if !is_saved {
            tracing::info!("Saving device per its request");
            library
                .add_device(push_token)
                .await
                .context("Couldn't save device to database")?;
        }
    }

    // Get all paths we care about
    let mut selected = Vec::new();
    for path in args.paths {
        if path.is_dir() {
            let spin = Progression::new_spinner(
                args.progress,
                format!("Finding music files for {}", path.display()),
            );
            spin.enable_steady_tick(Duration::from_millis(300));
            if args.recurse {
                let dir = path.clone();
                // Recursively get all paths, then find the ones with MIME types we care about
                let mut paths = tokio::task::spawn_blocking(move || get_dir_paths(&dir))
                    .await
                    .with_context(|| format!("while recursing {}", path.display()))??
                    .into_iter()
                    .filter_map(|p| {
                        mime_guess::from_path(&p)
                            .iter()
                            .find(|m| device.mime_supported(m))
                            .map(|mime| (p, mime))
                    })
                    .collect();
                selected.append(&mut paths);
            } else {
                tracing::warn!(
                    "skipping directory '{}' as -r was not defined",
                    path.display()
                );
            }
            spin.finish_and_clear();
        } else {
            let Some(mime) = mime_guess::from_path(&path)
                .iter()
                .find(|m| device.mime_supported(m))
            else {
                bail!("{}: unsupported mime type", path.display());
            };

            selected.push((path, mime));
        }
    }

    if selected.is_empty() {
        bail!("No music files were found");
    }

    let file_count = selected.len();
    tracing::info!("Uploading {} files", selected.len());

    let device = Arc::new(device);
    let (send, mut recv) = mpsc::channel::<anyhow::Error>(1);

    let progress = Progression::new(
        args.progress,
        file_count as u64,
        format!("Uploading {file_count} files"),
    );

    tokio::spawn(process_all_paths(
        device.clone(),
        selected,
        send,
        args.tasks as usize,
        progress.clone(),
    ));
    if let Some(err) = recv.recv().await {
        progress.abandon();
        Err(err)
    } else {
        progress.finish_and_clear();
        Ok(())
    }
}
