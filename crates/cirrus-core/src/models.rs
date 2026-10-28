use serde::{Deserialize, Serialize};

pub mod statuses {

    pub const OK: &str = "OK";
    pub const ERROR: &str = "ERROR";
    pub const SUCCESS: &str = "SUCCESS";

    pub const CHALLENGE: &str = "CHALLENGE";
    pub const TWO_FA_REQUIRED: &str = "2FA_REQUIRED";

    pub const NEED_2FA: &str = "NEED_2FA";

    pub const SUBMITTED: &str = "SUBMITTED";

    pub const CODE_SENT: &str = "CODE_SENT";
    pub const TRUST_BROWSER_REQUIRED: &str = "TRUST_BROWSER_REQUIRED";

    pub const AUTHENTICATED: &str = "AUTHENTICATED";
    pub const PENDING: &str = "PENDING";
}

pub mod session_keys {
    pub const SCNT: &str = "scnt";
    pub const SESSION_ID: &str = "session_id";
    pub const AUTH_ATTRIBUTES: &str = "auth_attributes";
    pub const SESSION_TOKEN: &str = "session_token";
    pub const TRUST_TOKEN: &str = "trust_token";
    pub const ACCOUNT_COUNTRY: &str = "account_country";
    pub const RSCD: &str = "rscd";
    pub const PUSH_TOKEN: &str = "push_token";

    pub const BOOTSTRAP_BODY: &str = "bootstrap_body";
    pub const CHALLENGE_BODY: &str = "challenge_body";

    pub const TWO_FACTOR_VERIFIED: &str = "two_factor_verified";
    pub const PHOTOS_VERIFIED: &str = "photos_2fa_verified";

    pub const PHOTOS_ACCESS_PROBE_AT: &str = "photos_access_probe_at";

    pub const PHOTOS_ZONE_OWNER: &str = "photos_zone_owner";

    pub const FMIP_CLIENT_ID: &str = "fmip_client_id";

    pub const FLAG_TRUE: &str = "true";
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialsRequest {
    pub apple_id: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRequest {
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaySoundRequest {
    pub session_id: String,
    pub device_id: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteFolder {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteSummary {
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub folder_id: Option<String>,
    pub created_at: Option<i64>,
    pub modified_at: Option<i64>,
    pub pinned: bool,
    pub deleted: bool,
    pub first_attachment_uti: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotesListResult {
    pub status: String,
    pub message: Option<String>,
    pub folders: Vec<NoteFolder>,
    pub notes: Vec<NoteSummary>,
    pub next_cursor: Option<String>,
}

impl NotesListResult {
    pub fn ok(folders: Vec<NoteFolder>, notes: Vec<NoteSummary>, next_cursor: Option<String>) -> Self {
        Self { status: statuses::OK.to_string(), message: None, folders, notes, next_cursor }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            status: statuses::ERROR.to_string(),
            message: Some(message.into()),
            folders: Vec::new(),
            notes: Vec::new(),
            next_cursor: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteAttachment {
    pub id: String,
    pub name: String,
    pub uti: Option<String>,
    pub preview_url: Option<String>,
    pub dark_preview_url: Option<String>,
    pub fallback_url: Option<String>,
    pub download_url: Option<String>,
    pub render_url: Option<String>,
    pub table: Option<Vec<Vec<String>>>,
    pub file_size: Option<u64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteDetail {
    pub id: String,
    pub title: String,
    pub body: String,
    pub body_html: String,
    pub folder_id: Option<String>,
    pub created_at: Option<i64>,
    pub modified_at: Option<i64>,
    pub attachments: Vec<NoteAttachment>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteDetailResult {
    pub status: String,
    pub message: Option<String>,
    pub note: Option<NoteDetail>,
}

impl NoteDetailResult {
    pub fn ok(note: NoteDetail) -> Self {
        Self { status: statuses::OK.to_string(), message: None, note: Some(note) }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self { status: statuses::ERROR.to_string(), message: Some(message.into()), note: None }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SrpAuthRequest {
    pub apple_id: String,
    pub password: String,
    #[serde(default)]
    pub china_mainland: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SrpInitResult {
    pub session_id: String,

    pub protocol: Option<String>,

    pub iteration: Option<u32>,

    pub salt: Option<String>,

    pub b: Option<String>,

    pub c: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SrpResult {
    pub status: String,
    pub message: Option<String>,
    pub session_id: Option<String>,

    pub auth_type: Option<String>,

    pub session_token: Option<String>,
    pub trust_token: Option<String>,

    pub apple_session_id: Option<String>,
    pub success: Option<bool>,

    pub details: serde_json::Value,
}

impl SrpResult {
    pub fn success(
        session_id: String,
        session_token: Option<String>,
        trust_token: Option<String>,
        apple_session_id: Option<String>,
        success: Option<bool>,
        details: serde_json::Value,
    ) -> Self {
        Self {
            status: statuses::SUCCESS.to_string(),
            message: None,
            session_id: Some(session_id),
            auth_type: None,
            session_token,
            trust_token,
            apple_session_id,
            success,
            details,
        }
    }

    pub fn challenge(
        session_id: String,
        auth_type: Option<String>,
        session_token: Option<String>,
        trust_token: Option<String>,
        apple_session_id: Option<String>,
        details: serde_json::Value,
    ) -> Self {
        Self {
            status: statuses::CHALLENGE.to_string(),
            message: None,
            session_id: Some(session_id),
            auth_type,
            session_token,
            trust_token,
            apple_session_id,
            success: None,
            details,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLocation {
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub horizontal_accuracy: Option<f64>,
    pub time_stamp: Option<i64>,

    pub is_old: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub name: Option<String>,
    pub model_display_name: Option<String>,
    pub device_display_name: Option<String>,
    pub device_class: Option<String>,
    pub serial_number: Option<String>,
    pub udid: Option<String>,
    pub os_version: Option<String>,
    pub battery_status: Option<String>,

    pub battery_level: Option<f64>,
    pub location: Option<DeviceLocation>,

    pub device_image_url: Option<String>,

    pub device_id: Option<String>,
    pub activation_locked: Option<bool>,
    pub lost_mode_enabled: Option<bool>,

    pub lost_mode_pending: Option<bool>,

    pub location_enabled: Option<bool>,

    pub device_status: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfoResult {
    pub status: String,
    pub message: Option<String>,
    pub dsid: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub device_count: i32,
    pub devices: Vec<Device>,
}

impl DeviceInfoResult {
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            status: statuses::ERROR.to_string(),
            message: Some(message.into()),
            dsid: None,
            first_name: None,
            last_name: None,
            device_count: 0,
            devices: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaySoundResult {
    pub success: bool,
    pub message: Option<String>,
}

impl PlaySoundResult {
    pub fn error(message: impl Into<String>) -> Self {
        Self { success: false, message: Some(message.into()) }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStateResponse {
    pub valid: bool,
    pub verified: bool,
    pub first_name: Option<String>,
    pub last_name: Option<String>,

    pub apple_id: Option<String>,

    pub dsid: Option<String>,
    pub ttl_minutes: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountAvatar {
    pub data_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotosLoginResult {
    pub status: String,
    pub message: Option<String>,
    pub session_id: Option<String>,
    pub auth_type: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

impl PhotosLoginResult {
    pub fn need_2fa(session_id: String, auth_type: Option<String>) -> Self {
        Self {
            status: statuses::NEED_2FA.to_string(),
            message: Some(
                "Two-factor authentication is enabled. Enter the 6-digit code shown on your trusted device."
                    .to_string(),
            ),
            session_id: Some(session_id),
            auth_type,
            first_name: None,
            last_name: None,
        }
    }

    pub fn trust_browser_required(session_id: String) -> Self {
        Self {
            status: statuses::TRUST_BROWSER_REQUIRED.to_string(),
            message: Some("Verification succeeded. Choose whether to trust this browser.".to_string()),
            session_id: Some(session_id),
            auth_type: None,
            first_name: None,
            last_name: None,
        }
    }

    pub fn ok(session_id: String, first_name: Option<String>, last_name: Option<String>) -> Self {
        Self {
            status: statuses::OK.to_string(),
            message: None,
            session_id: Some(session_id),
            auth_type: None,
            first_name,
            last_name,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            status: statuses::ERROR.to_string(),
            message: Some(message.into()),
            session_id: None,
            auth_type: None,
            first_name: None,
            last_name: None,
        }
    }

    pub fn status_only(status: &str, message: Option<String>) -> Self {
        Self {
            status: status.to_string(),
            message,
            session_id: None,
            auth_type: None,
            first_name: None,
            last_name: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoItem {
    pub id: String,
    pub file_name: Option<String>,

    pub taken_at: Option<i64>,
    pub is_video: Option<bool>,

    pub thumb_url: String,

    pub display_url: Option<String>,

    pub original_url: Option<String>,

    pub video_url: Option<String>,

    pub is_live_photo: Option<bool>,

    pub width: Option<i64>,

    pub height: Option<i64>,

    pub size_bytes: Option<i64>,

    pub added_at: Option<i64>,
    pub duration_seconds: Option<f64>,
    pub frame_rate: Option<f64>,
    pub codec: Option<String>,
    pub time_zone_offset_seconds: Option<i64>,
    pub is_favorite: Option<bool>,
    pub is_hidden: Option<bool>,
    pub keywords: Vec<String>,
    pub metadata: Option<PhotoMetadata>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoMetadata {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub lens_specification: Option<String>,

    pub date_taken: Option<String>,
    pub exposure_time: Option<String>,
    pub f_number: Option<String>,
    pub iso: Option<String>,
    pub focal_length: Option<String>,
    pub focal_length_35mm: Option<String>,
    pub exposure_bias: Option<String>,

    pub metering_mode: Option<String>,
    pub flash: Option<String>,
    pub white_balance: Option<String>,
    pub color_space: Option<String>,
    pub software: Option<String>,

    pub gps_latitude: Option<f64>,
    pub gps_longitude: Option<f64>,
    pub gps_altitude: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotosPage {
    pub status: String,
    pub message: Option<String>,
    pub photos: Vec<PhotoItem>,

    pub next_cursor: Option<String>,
}

impl PhotosPage {
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            status: statuses::ERROR.to_string(),
            message: Some(message.into()),
            photos: Vec::new(),
            next_cursor: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryTotal {
    pub photos: i64,
    pub videos: i64,

    pub assets: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetDistributionDto {
    pub bucket_width_ms: i64,

    pub total: i64,

    pub buckets: Vec<AssetBucketDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetBucketDto {
    pub start_ms: i64,

    pub count: i64,

    pub start_rank: i64,

    pub second_month_rank: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineDto {
    pub total: i64,

    pub years: Vec<YearEntryDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YearEntryDto {
    pub year: i32,

    pub count: i64,

    pub months: Vec<MonthEntryDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthEntryDto {
    pub year: i32,

    pub month: u32,

    pub count: i64,

    pub start_rank: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotosTimelineDto {
    pub distribution: AssetDistributionDto,
    pub timeline: TimelineDto,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotosTimelineRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthExactDto {
    pub year: i32,

    pub month: u32,

    pub count: i64,

    pub newest_rank: i64,

    pub oldest_rank: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotosMonthExactRequest {
    pub session_id: String,
    pub year: i32,

    pub month: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotosListRequest {
    pub session_id: String,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<i32>,

    #[serde(default)]
    pub start_rank: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneOption {
    pub id: i64,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Photos2faOptions {
    pub status: String,
    pub message: Option<String>,
    pub trusted_device: bool,
    pub trusted_device_count: i32,
    pub phones: Vec<PhoneOption>,
}

impl Photos2faOptions {
    pub fn ok(trusted_device_count: i32, phones: Vec<PhoneOption>) -> Self {
        Self {
            status: statuses::OK.to_string(),
            message: None,
            trusted_device: trusted_device_count != 0,
            trusted_device_count,
            phones,
        }
    }

    pub fn unknown(trusted_device_count: i32, phones: Vec<PhoneOption>) -> Self {
        Self { status: statuses::OK.to_string(), message: None, trusted_device: true, trusted_device_count, phones }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            status: statuses::ERROR.to_string(),
            message: Some(message.into()),
            trusted_device: false,
            trusted_device_count: 0,
            phones: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotosVerifyRequest {
    pub session_id: String,
    pub code: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotosFinish2faRequest {
    pub session_id: String,
    pub trust_browser: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotosPhoneRequest {
    pub session_id: String,
    pub phone_number_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotosPhoneVerifyRequest {
    pub session_id: String,
    pub phone_number_id: i64,
    pub code: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoDownloadRequest {
    pub url: String,

    #[serde(default)]
    pub name: Option<String>,

    #[serde(default)]
    pub target_dir: Option<String>,
}
