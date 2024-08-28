use std::path::Path;

use mime::Mime;
use reqwest::multipart;

use crate::{error::ApiError, model};

/// A connection to a Doppler device.
pub struct DeviceClient {
    http_client: reqwest::Client,
    info: model::DeviceInfo,
    base_uri: reqwest::Url,
    push_token: Option<model::Device>,
}

impl DeviceClient {
    /// Creates a new DeviceClient from the given LAN URL.
    pub(crate) async fn new(
        uri: impl AsRef<str>,
        push_token: Option<model::Device>,
    ) -> crate::Result<Self> {
        let base_uri = reqwest::Url::parse(uri.as_ref())?;
        let http_client = reqwest::Client::new();
        let info: model::DeviceInfo = http_client
            .get(base_uri.join("info").unwrap())
            .send()
            .await?
            .json()
            .await?;
        Ok(Self {
            http_client,
            info,
            base_uri,
            push_token,
        })
    }

    /// Returns a list of all MIME types reported as supported by the device.
    pub fn supported_mimetypes(&self) -> &[String] {
        &self.info.supported_mimetypes
    }

    /// If the device requested to be saved, provides the device metadata
    /// represented as the "push token" by the Doppler API.
    pub fn push_token(&self) -> Option<&model::Device> {
        self.push_token.as_ref()
    }

    /// Checks whether the given `Mime` is supported by the device.
    ///
    /// # Examples
    ///
    /// Using [`mime_guess`] with the file path:
    ///
    /// ```no_run
    /// use tokio::fs::File;
    ///
    /// let filename = "cool_tapes.mp3";
    ///
    /// // Iterate through all guessed MIME types, checking if any are supported
    /// if mime_guess::from_path(filename)
    ///     .iter()
    ///     .any(|mime| client.mime_supported(mime))
    /// {
    ///     // Supported by device!
    /// }
    /// ```
    pub fn mime_supported(&self, mime: &Mime) -> bool {
        if self
            .info
            .supported_mimetypes
            .iter()
            .any(|mt| mt == mime.essence_str())
        {
            true
        } else {
            // Try with the x- prefixed version of the mimetype
            let x_mime = format!("{}/x-{}", mime.type_(), mime.subtype());
            self.info.supported_mimetypes.iter().any(|mt| x_mime.eq(mt))
        }
    }

    /// Returns a list of all file extensions reported as known by the device.
    pub fn supported_extensions(&self) -> &[String] {
        &self.info.known_file_extensions
    }

    /// Checks whether the given file path has a supported file extension.
    pub fn extension_supported(&self, path: impl AsRef<Path>) -> bool {
        if let Some(path_ext) = path.as_ref().extension() {
            self.info
                .known_file_extensions
                .iter()
                .any(|ext| ext.as_bytes() == path_ext.as_encoded_bytes())
        } else {
            false
        }
    }

    /// Uploads a file to the device.
    ///
    /// While not enforced by this function, the MIME type and file extension
    /// should be checked before uploading.
    pub async fn upload(
        &self,
        filename: impl AsRef<Path>,
        len: u64,
        mime: Mime,
        data: impl Into<reqwest::Body>,
    ) -> super::Result<()> {
        let basename = filename
            .as_ref()
            .file_name()
            .ok_or(ApiError::InvalidPath)?
            .to_string_lossy()
            .to_string();
        let form = multipart::Form::new()
            .part("filename", multipart::Part::text(basename.clone()))
            .part(
                "file",
                multipart::Part::stream_with_length(data, len)
                    .file_name(basename)
                    .mime_str(mime.as_ref())
                    .unwrap(),
            );
        let response = self
            .http_client
            .post(self.base_uri.join("upload").unwrap())
            .multipart(form)
            .send()
            .await?;

        let _ = response.bytes().await?;
        Ok(())
    }
}
