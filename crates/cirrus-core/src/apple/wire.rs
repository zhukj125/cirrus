use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SigninInitRequest {
    pub a: String,

    pub account_name: String,

    pub protocols: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SigninInitResponse {
    pub salt: Option<String>,

    pub b: Option<String>,

    pub c: Option<String>,

    pub iteration: Option<u32>,

    pub protocol: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SigninCompleteRequest {
    pub account_name: String,
    pub c: String,

    pub m1: String,

    pub m2: String,
    pub remember_me: bool,
    pub trust_tokens: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SigninChallengeResponse {
    pub auth_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientContext {
    pub app_name: &'static str,
    pub app_version: &'static str,
    pub api_version: &'static str,
    pub device_list_version: i32,
    pub fmly: bool,
    pub timezone: &'static str,
    pub inactive_time: i64,
}

impl ClientContext {
    pub fn icloud_find_web() -> Self {
        Self {
            app_name: "iCloud Find (Web)",
            app_version: "2.0",
            api_version: "3.0",
            device_list_version: 1,
            fmly: true,
            timezone: "US/Pacific",
            inactive_time: 1313,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountLoginRequest {
    pub account_country_code: String,
    pub ds_web_auth_token: Option<String>,
    pub extended_login: bool,
    pub trust_token: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeCardResponse {
    pub contacts: Option<Vec<MeCardContact>>,
}

impl MeCardResponse {
    pub fn photo_url(&self) -> Option<&str> {
        self.contacts
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .find_map(|contact| contact.photo.as_ref().and_then(|photo| photo.url.as_deref()))
            .filter(|url| !url.trim().is_empty())
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeCardContact {
    pub photo: Option<MeCardPhoto>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeCardPhoto {
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitClientRequest {
    pub client_context: ClientContext,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FmipLocation {
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub horizontal_accuracy: Option<f64>,
    pub time_stamp: Option<i64>,

    pub is_old: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FmipDevice {
    pub name: Option<String>,
    pub model_display_name: Option<String>,
    pub device_display_name: Option<String>,
    pub device_class: Option<String>,
    pub serial_number: Option<String>,
    pub udid: Option<String>,
    pub os_version: Option<String>,
    pub battery_status: Option<String>,

    pub battery_level: Option<f64>,
    pub location: Option<FmipLocation>,

    pub raw_device_model: Option<String>,
    pub device_color: Option<String>,

    pub id: Option<String>,
    pub activation_locked: Option<bool>,
    pub lost_mode_enabled: Option<bool>,

    pub lost_device: Option<serde_json::Value>,
    pub location_enabled: Option<bool>,

    pub device_status: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitClientResponse {
    pub content: Option<Vec<FmipDevice>>,

    pub server_context: Option<serde_json::Value>,
}

impl InitClientResponse {
    pub fn devices(&self) -> &[FmipDevice] {
        self.content.as_deref().unwrap_or(&[])
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ServiceErrorResponse {
    #[serde(rename = "errorMessage")]
    pub error_message: Option<String>,
    pub reason: Option<String>,
    pub error: Option<String>,
    pub message: Option<String>,
    #[serde(rename = "errorCode")]
    pub error_code: Option<String>,
    #[serde(rename = "serverErrorCode")]
    pub server_error_code: Option<String>,
}

impl ServiceErrorResponse {
    pub fn message(&self) -> Option<&str> {
        [self.error_message.as_deref(), self.reason.as_deref(), self.error.as_deref(), self.message.as_deref()]
            .into_iter()
            .flatten()
            .next()
    }

    pub fn code(&self) -> Option<&str> {
        [self.error_code.as_deref(), self.server_error_code.as_deref()].into_iter().flatten().next()
    }
}
