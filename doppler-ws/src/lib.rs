//! Provides access to the unofficial (private) Doppler Wi-Fi Transfer APIs.
//!
//! # Important information about stability
//!
//! This uses private, undocumented functionality used as part of the
//! [doppler-transfer.com] website. These APIs are subject to change without
//! warning.
//!
//! As the API has been reverse engineered, any documentation pertaining to the
//! actual API (such as request/response payloads) may be inaccurate or missing.
//!
//! # Pairing
//!
//! There are two pairing methods: using a saved device, or with the pairing
//! code. Pairing with a saved device can only be done when the device
//! information was saved. Said information is referred internally as the
//! `push_token`.
//!
//! ## Pairing with the code
//!
//! To pair with a device using the pairing code:
//!
//! ```no_run
//! let client = TransferClient::connect().await?;
//!
//! // This can be given to the user as-is and/or as a QR Code
//! let pairing_code = client.code();
//! println!("Use {pairing_code} in your app to connect.");
//!
//! // Wait for the user to enter the code
//! let mut response = client.get_new_device().await?;
//!
//! // Check whether the device is saved. Storage of devices should be handled
//! // by your application. This is not strictly required, but is how the
//! // website handles it.
//! let is_saved = do_we_have_device_id(response.id());
//!
//! // Paired! Now we're connected directly to the device.
//! let device = client.confirm_device(&mut response, is_saved).await?;
//! ```
//!
//! ## Pairing with a saved device
//!
//! If you have a [`Device`] saved (for example, serialized in a database), you
//! can initiate the pairing process without needing to show the code to the user:
//!
//! ```no_run
//! let client = TransferClient::connect().await?;
//! // Pull the Device object from your database. `Device` derives
//! // Serialize/Deserialize, so you can use your favorite serde crate to store
//! // it.
//! let our_device: Device = get_device_from_database_somehow();
//!
//! // This sends a push notification to the user's device asking to open the
//! // app so we can connect. This function will return once that's done.
//! let mut response = client.get_saved_device(our_device).await?;
//!
//! // And that's it!
//! let device = client.confirm_device(&mut response, is_saved).await?;
//! ```
//!
//! [doppler-transfer.com]: https://doppler-transfer.com

use error::ApiError;
use futures_util::{SinkExt, TryStreamExt};
use model::Device;
use tokio::net::TcpStream;
use tokio_websockets::{MaybeTlsStream, Message, WebSocketStream};

pub mod device;
pub mod error;
pub mod model;

pub type Result<T> = std::result::Result<T, ApiError>;

const API_DOMAIN: &str = "doppler-transfer.com";

/// A connection to the Wi-Fi Transfer API. This is used solely for pairing.
pub struct TransferClient {
    http_client: reqwest::Client,
    ws_client: WebSocketStream<MaybeTlsStream<TcpStream>>,
    code: String,
    msg_queue: Vec<model::ApiResponse>,
}

// Pulls the actual API response we want out of the ApiResponse enum
macro_rules! get_response {
    ($self:tt, $rty:ident) => {{
        let $crate::model::ApiResponse::$rty(val) = $self
            .next_msg(|r| matches!(r, $crate::model::ApiResponse::$rty(_)))
            .await?
        else {
            unreachable!();
        };

        val
    }};
}

impl TransferClient {
    /// Connects to the Doppler Transfer API.
    pub async fn connect() -> Result<Self> {
        use tokio_websockets::ClientBuilder;

        let random_id = uuid::Uuid::new_v4();
        let doppler_url = http::Uri::builder()
            .scheme("wss")
            .authority(API_DOMAIN)
            .path_and_query(format!("/api/v1/code?id={random_id}"))
            .build()
            .unwrap();
        let (ws_client, _) = ClientBuilder::from_uri(doppler_url).connect().await?;

        let mut new_self = Self {
            http_client: reqwest::Client::new(),
            ws_client,
            code: String::new(), // placeholder
            msg_queue: Vec::new(),
        };

        let code_data = get_response!(new_self, Code);
        new_self.code = code_data.code;

        Ok(new_self)
    }

    /// Returns a reference to the device pairing code.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Get the next text message.
    async fn next_msg(
        &mut self,
        filter: impl Fn(&model::ApiResponse) -> bool,
    ) -> Result<model::ApiResponse> {
        // First, see if we already received a message of the given filter
        if let Some(idx) = self.msg_queue.iter().position(&filter) {
            Ok(self.msg_queue.remove(idx))
        } else {
            while let Some(msg) = self.ws_client.try_next().await? {
                if let Some(text) = msg.as_text() {
                    let response: model::ApiResponse = serde_json::from_str(text)?;
                    if filter(&response) {
                        return Ok(response);
                    } else {
                        // Not our message, add it to the queue and loop
                        self.msg_queue.push(response);
                    }
                }
            }
            // Stream ended?
            Err(ApiError::Io(std::io::ErrorKind::UnexpectedEof.into()))
        }
    }

    /// Completes the pairing process. If successful, a `DeviceClient` is
    /// returned.
    ///
    /// If the device was already saved, set `is_saved` to true.
    pub async fn confirm_device(
        &mut self,
        device: &mut model::DeviceResponse,
        is_saved: bool,
    ) -> Result<device::DeviceClient> {
        device.is_saved = Some(is_saved);
        let str_response = serde_json::to_string(&device)?;
        self.ws_client.send(Message::text(str_response)).await?;
        let lan_url = get_response!(self, LanUrl);
        device::DeviceClient::new(&lan_url.url_lan, lan_url.push_token).await
    }

    /// Waits for a device to pair with the pairing code.
    pub async fn get_new_device(&mut self) -> Result<model::DeviceResponse> {
        Ok(get_response!(self, Device))
    }

    /// Initiates the pairing process with a saved device by sending it a push
    /// notification.
    pub async fn get_saved_device(&mut self, device: &Device) -> Result<model::DeviceResponse> {
        let Some(device_id) = &device.id else {
            return Err(ApiError::DeviceIdMissing);
        };

        let req = model::SpecificDeviceRequest {
            code: self.code.clone(),
            push_token: device.for_request(),
        };

        let response = self
            .http_client
            .post(format!("https://{API_DOMAIN}/api/v0/request-device"))
            .json(&req)
            .send()
            .await?;
        let status = response.status();
        // Workaround for current functionality
        if status.is_success() || status.as_u16() == 500 {
            let next_device = get_response!(self, Device);
            if next_device.id.eq(device_id) {
                // This is ours!
                Ok(next_device)
            } else {
                // TODO: Should we throw an error or just ignore it?
                Err(ApiError::UnexpectedDevice)
            }
        } else {
            Err(ApiError::BadResponse(response.status()))
        }
    }
}
