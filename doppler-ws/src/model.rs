use serde::{Deserialize, Serialize};

/// Response when a pairing code is requested.
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct CodeResponse {
    pub code: String,
}

/// Represents a device.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Device {
    pub name: Option<String>,
    pub id: Option<String>,
    pub user: String,
    pub device: String,
}

impl Device {
    /// Creates a "token" version of the device for use as the push token.
    pub(crate) fn for_request(&self) -> Self {
        Self {
            name: None,
            id: None,
            user: self.user.clone(),
            device: self.device.clone(),
        }
    }
}

// ------ API Responses ------

/// Represents all of the responses we might get from the API server.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum ApiResponse {
    /// Should only be received on connect. Represents the code used to pair our
    /// program with the user's device.
    Code(CodeResponse),
    /// Represents a device that used the provided code to request pairing with
    /// us.
    Device(DeviceResponse),
    /// Represents the LAN URL for the user's device, along with a potential
    /// push token.
    LanUrl(LanUrlResponse),
}

/// Represents a candidate Doppler device to pair with.
///
/// To confirm this device should be used, use the `TransferClient::confirm_device` function.
///
#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceResponse {
    #[serde(rename = "type")]
    pub(crate) device_type: String,
    #[serde(rename = "device")]
    pub(crate) id: String,
    pub(crate) is_saved: Option<bool>,
}

impl DeviceResponse {
    /// Get the reported device ID. This is primarily used to confirm the device
    /// has been saved.
    pub fn id(&self) -> &str {
        &self.id
    }
}

/// Represents the LAN URL for the user's device.
///
/// If `push_token` is not `None`, the device has requested we save a record of
/// it. This can be used to generate a push notification that we'd like to
/// connect in a future session.
///
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct LanUrlResponse {
    pub(crate) url_lan: String,
    pub(crate) push_token: Option<Device>,
}

// ------ API Requests ------

/// Request payload for /api/v0/request-device.
#[derive(Debug, Serialize)]
pub(crate) struct SpecificDeviceRequest {
    pub code: String,
    pub push_token: Device,
}

// ------ Device API Responses ------

// Meta-information returned from the device.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
// Allowing since this is relevant API schema data, even if we aren't using it
// right now.
#[allow(dead_code)]
pub(crate) struct DeviceInfo {
    pub(crate) device_name: String,
    pub(crate) known_file_extensions: Vec<String>,
    pub(crate) supported_mimetypes: Vec<String>,
    pub(crate) app_name: String,
    pub(crate) app_version: u32,
}
