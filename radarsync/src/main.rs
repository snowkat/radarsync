mod db;

use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

use anyhow::{bail, Context};
use clap::Parser;
use db::Library;
use doppler_ws::device::DeviceClient;
use mime_guess::Mime;
use tracing::level_filters::LevelFilter;

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
    /// Sync to a saved device
    #[arg(short, long)]
    device: Option<String>,
    /// Disable the QR Code display
    #[arg(long)]
    no_qr: bool,
    /// Paths to transfer to the device
    #[arg(required = true)]
    paths: Vec<PathBuf>,
}

// Wrapper for app_main
fn main() -> ExitCode {
    let args = Args::parse();

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
) -> anyhow::Result<()> {
    tracing::info!("Uploading {}", path.as_ref().display());
    let file = tokio::fs::File::open(path).await?;

    let meta = file.metadata().await?;
    device.upload(path, meta.len(), mime, file).await?;

    Ok(())
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

    let mut response = if let Some(device) = args.device {
        // Perform the saved device pairing flow
        let Some(device) = library.get_device(&device).await? else {
            bail!("Device name not found");
        };
        api.get_saved_device(&device).await
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

    tracing::info!("Uploading {} files", selected.len());

    for (path, mime) in selected {
        process_file(&device, mime, &path)
            .await
            .with_context(|| format!("{}", path.display()))?;
    }

    Ok(())
}
