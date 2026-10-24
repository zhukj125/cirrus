use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use reqwest::Method;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::apple::api;
use crate::apple::http::{client as hc, headers as hh};
use crate::apple::wire::{AccountLoginRequest, ClientContext, FmipDevice, InitClientRequest, InitClientResponse};
use crate::error::{AppError, AppResult};
use crate::models::session_keys::*;
use crate::models::{statuses, Device, DeviceInfoResult, DeviceLocation, PlaySoundResult};
use crate::session::{SessionHandle, SrpSession};

fn web_auth_token(session: &SrpSession) -> Option<String> {
    session.session_data(SESSION_TOKEN).map(str::trim).filter(|value| !value.is_empty()).map(str::to_string)
}

pub async fn list_devices(handle: &SessionHandle) -> DeviceInfoResult {
    let mut session = handle.lock().await;

    if let Err(error) = account_login(&mut session, false).await {
        log::warn!("[DEVICE] accountLogin failed: {error}");
        return DeviceInfoResult::error(error.to_string());
    }
    if session.dsid().is_none() {
        return DeviceInfoResult::error("accountLogin did not return a dsid; devices cannot be queried");
    }
    let client_id = session
        .session_data(FMIP_CLIENT_ID)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    session.set_session_data(FMIP_CLIENT_ID, &client_id);

    if session.server_context().is_some() {
        match find_my_refresh(&mut session).await {
            Ok(result) if result.status == statuses::SUCCESS => {
                handle.touch();
                return result;
            }
            Ok(result) => log::warn!("[DEVICE] refreshClient failed: {}", result.message.unwrap_or_default()),
            Err(error) => log::warn!("[DEVICE] refreshClient failed; falling back to initClient: {error}"),
        }
    }

    match find_my_init(&mut session).await {
        Ok(result) => {
            handle.touch();
            result
        }
        Err(error) => {
            log::warn!("[DEVICE] initClient failed: {error}");
            DeviceInfoResult::error(error.to_string())
        }
    }
}
// ---- Step 1: accountLogin ----

pub async fn account_login(session: &mut SrpSession, force: bool) -> AppResult<()> {
    if !force && session.dsid().is_some() && session.contacts_url().is_some() {
        return Ok(());
    }
    if force {
        session.clear_cloud_session();
    }

    for _attempt in 0..2 {
        let country = session
            .session_data(ACCOUNT_COUNTRY)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(hc::DEFAULT_ACCOUNT_COUNTRY)
            .to_string();
        let payload = AccountLoginRequest {
            account_country_code: country,
            ds_web_auth_token: web_auth_token(session),
            extended_login: true,
            trust_token: session.session_data(TRUST_TOKEN).map(str::to_string),
        };
        let payload = serde_json::to_value(&payload).map_err(AppError::from)?;

        let owned_headers = session.oauth_headers();
        let headers: Vec<(&str, &str)> = owned_headers.iter().map(|(name, value)| (*name, value.as_str())).collect();
        let url = format!("{}{}", session.setup_base(), api::setup::ACCOUNT_LOGIN);
        let response = session.request(Method::POST, &url, Some(&payload), &headers).await?;

        if response.status == 302 {
            let data = response.json();
            let domain = data.get(api::json::DOMAIN_TO_USE).and_then(Value::as_str).unwrap_or("");
            if domain.to_ascii_lowercase().contains("cn") {
                log::info!("[DEVICE] detected China region ({domain}); switching server");
                session.switch_to_china();
                continue;
            }
        }
        if response.status != 200 {
            return Err(AppError::auth(format!(
                "accountLogin failed: HTTP {} {}",
                response.status,
                short(&response.body, 300)
            )));
        }

        let data = response.json();
        let Some(dsid) = data.pointer(&format!("/{}/{}", api::json::DS_INFO, api::json::DSID)).and_then(Value::as_str)
        else {
            return Err(AppError::auth(format!(
                "accountLogin response is missing dsInfo.dsid: {}",
                short(&response.body, 300)
            )));
        };

        session.set_dsid(Some(dsid.to_string()));
        session.set_first_name(text_pointer(&data, api::json::DS_INFO, api::json::FIRST_NAME));
        session.set_last_name(text_pointer(&data, api::json::DS_INFO, api::json::LAST_NAME));

        session.set_find_me_url(service_url(&data, api::json::SERVICE_FINDME));
        session.set_photos_url(service_url(&data, api::json::SERVICE_PHOTOS));
        session.set_cloud_kit_url(service_url(&data, api::json::SERVICE_CKDATABASEWS));
        session.set_contacts_url(service_url(&data, api::json::SERVICE_CONTACTS));

        log::info!(
            "[DEVICE] accountLogin succeeded (dsid={:?}, findme={:?}, ckdatabasews={:?})",
            session.dsid(),
            session.find_me_url(),
            session.cloud_kit_url()
        );
        return Ok(());
    }
    Err(AppError::auth("accountLogin failed: sign-in did not complete after the domain redirect"))
}

// ---- Step 3: refreshClient / initClient ----

async fn find_my_refresh(session: &mut SrpSession) -> AppResult<DeviceInfoResult> {
    let url = find_my_service_url(session, api::find_my::REFRESH_CLIENT)?;
    let context = serde_json::to_value(ClientContext::icloud_find_web()).map_err(AppError::from)?;
    let mut payload = json!({ api::json::CLIENT_CONTEXT: context });
    if let Some(server_context) = session.server_context().cloned() {
        if let Value::Object(ref mut map) = payload {
            map.insert(api::json::SERVER_CONTEXT.to_string(), server_context);
        }
    }
    let headers = [(hh::CONTENT_TYPE, hc::CONTENT_TYPE_TEXT_PLAIN), (hh::ACCEPT, "*/*")];
    let response = session.request(Method::POST, &url, Some(&payload), &headers).await?;
    parse_device_response(session, "refreshClient", response.status, &response.body)
}

async fn find_my_init(session: &mut SrpSession) -> AppResult<DeviceInfoResult> {
    let url = find_my_service_url(session, api::find_my::INIT_CLIENT)?;
    let payload = InitClientRequest { client_context: ClientContext::icloud_find_web() };
    let payload = serde_json::to_value(&payload).map_err(AppError::from)?;
    let headers = [(hh::CONTENT_TYPE, hc::CONTENT_TYPE_TEXT_PLAIN), (hh::ACCEPT, "*/*")];
    let response = session.request(Method::POST, &url, Some(&payload), &headers).await?;
    parse_device_response(session, "initClient", response.status, &response.body)
}

fn find_my_service_url(session: &SrpSession, path: &str) -> AppResult<String> {
    let Some(base) = session.find_me_url().map(str::trim).filter(|url| !url.is_empty()) else {
        return Err(AppError::auth("Find My service URL was not returned (webservices.findme.url is missing)"));
    };
    let Some(dsid) = session.dsid() else {
        return Err(AppError::auth("Session is missing a dsid; devices cannot be queried"));
    };

    let Some(client_id) = session.session_data(FMIP_CLIENT_ID).filter(|value| !value.trim().is_empty()) else {
        return Err(AppError::auth("Find My clientId is not initialized"));
    };
    let client_id = utf8_percent_encode(client_id, NON_ALPHANUMERIC).to_string();
    let build = utf8_percent_encode(hc::FIND_MY_CLIENT_BUILD_NUMBER, NON_ALPHANUMERIC).to_string();
    Ok(format!(
        "{}{path}?clientBuildNumber={build}&clientMasteringNumber={build}&clientId={client_id}&dsid={dsid}",
        base.trim_end_matches('/')
    ))
}

fn parse_device_response(
    session: &mut SrpSession,
    action: &str,
    status: u16,
    body: &str,
) -> AppResult<DeviceInfoResult> {
    if status != 200 {
        return Err(AppError::auth(format!("{action} failed: HTTP {status} {}", short(body, 300))));
    }

    let data: InitClientResponse = serde_json::from_str(body)
        .map_err(|error| AppError::auth(format!("Could not parse the {action} response: {error}")))?;

    let server_context = data.server_context.clone();
    if let Some(context) = server_context.as_ref() {
        if context.is_object() {
            session.set_server_context(Some(context.clone()));
        }
    }

    let image_base = server_context
        .as_ref()
        .and_then(|context| context.get(api::json::IMAGE_BASE_URL))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let find_me_url = session.find_me_url().map(str::to_string);
    let devices: Vec<Device> =
        data.devices().iter().map(|device| to_device(find_me_url.as_deref(), device, &image_base)).collect();

    log::info!("[DEVICE] loaded {} devices (dsid={:?})", devices.len(), session.dsid());
    Ok(DeviceInfoResult {
        status: statuses::SUCCESS.to_string(),
        message: None,
        dsid: session.dsid().map(str::to_string),
        first_name: session.first_name().map(str::to_string),
        last_name: session.last_name().map(str::to_string),
        device_count: devices.len() as i32,
        devices,
    })
}

pub fn to_device(find_me_url: Option<&str>, source: &FmipDevice, image_base: &str) -> Device {
    let location = source.location.as_ref().map(|loc| DeviceLocation {
        latitude: loc.latitude,
        longitude: loc.longitude,
        horizontal_accuracy: loc.horizontal_accuracy,
        time_stamp: loc.time_stamp,
        is_old: loc.is_old,
    });

    Device {
        name: source.name.clone(),
        model_display_name: source.model_display_name.clone(),
        device_display_name: source.device_display_name.clone(),
        device_class: source.device_class.clone(),
        serial_number: source.serial_number.clone(),
        udid: source.udid.clone(),
        os_version: source.os_version.clone(),
        battery_status: source.battery_status.clone(),
        battery_level: source.battery_level,
        location,
        device_image_url: resolve_device_image(find_me_url, source, image_base),
        device_id: source.id.clone(),
        activation_locked: Some(source.activation_locked.unwrap_or(false)),
        lost_mode_enabled: Some(source.lost_mode_enabled.unwrap_or(false)),
        lost_mode_pending: Some(source.lost_device.as_ref().map(Value::is_object).unwrap_or(false)),
        location_enabled: Some(source.location_enabled.unwrap_or(true)),
        device_status: source.device_status.clone(),
    }
}

fn resolve_device_image(find_me_url: Option<&str>, device: &FmipDevice, image_base: &str) -> Option<String> {
    let device_class = device.device_class.as_deref().unwrap_or("");
    let raw_model = device.raw_device_model.as_deref().unwrap_or("");
    let device_color = device.device_color.as_deref().unwrap_or("");
    if device_class.trim().is_empty() || raw_model.trim().is_empty() {
        return None;
    }
    let model =
        if device_color.trim().is_empty() { raw_model.to_string() } else { format!("{raw_model}-{device_color}") };

    let online = device.location.as_ref().map(|loc| !loc.is_old.unwrap_or(false)).unwrap_or(false);
    let variant = if online { api::find_my::IMAGE_VARIANT_ONLINE } else { api::find_my::IMAGE_VARIANT_OFFLINE };

    let base = if image_base.trim().is_empty() {
        match find_me_url {
            Some(url) if url.contains(".cn") => api::find_my::IMAGE_CDN_CN.to_string(),
            _ => api::find_my::IMAGE_CDN.to_string(),
        }
    } else {
        format!("{}{}", image_base.trim_end_matches('/'), api::find_my::IMAGE_PATH)
    };
    Some(format!("{base}/{device_class}/{model}/{variant}"))
}

pub async fn play_sound(handle: &SessionHandle, device_id: &str) -> PlaySoundResult {
    let mut session = handle.lock().await;

    if session.dsid().is_none() || session.find_me_url().is_none() {
        return PlaySoundResult::error("The device service is not initialized. Load devices first.");
    }
    let Some(server_context) = session.server_context().cloned() else {
        return PlaySoundResult::error("Missing serverContext. Load devices first.");
    };
    if device_id.trim().is_empty() {
        return PlaySoundResult::error("Missing device ID");
    }

    let find_me_url = session.find_me_url().unwrap_or_default().to_string();
    let dsid = session.dsid().unwrap_or_default().to_string();
    let client_id = session.client_id().to_string();
    let build = utf8_percent_encode(hc::CLIENT_BUILD_NUMBER, NON_ALPHANUMERIC).to_string();
    let client_id_encoded = utf8_percent_encode(&client_id, NON_ALPHANUMERIC).to_string();

    let url = format!(
        "{find_me_url}{play}{q_build}{build}{q_master}{build}{q_client}{client_id}{q_dsid}{dsid}",
        play = api::find_my::PLAY_SOUND,
        q_build = api::find_my::QUERY_CLIENT_BUILD_NUMBER,
        q_master = api::find_my::QUERY_CLIENT_MASTERING_NUMBER,
        q_client = api::find_my::QUERY_CLIENT_ID,
        q_dsid = api::find_my::QUERY_DSID,
        client_id = client_id_encoded,
    );

    let client_context = serde_json::to_value(ClientContext::icloud_find_web()).unwrap_or(Value::Null);
    let body = json!({
        api::json::SERVER_CONTEXT: server_context,
        api::json::CLIENT_CONTEXT: client_context,
        api::json::DEVICE: device_id,
        api::json::SUBJECT: api::find_my::SUBJECT_PLAY_SOUND,
    });

    let mut owned_headers = session.auth_headers();
    owned_headers.push((hh::CONTENT_TYPE, hc::CONTENT_TYPE_TEXT_PLAIN.to_string()));
    let headers: Vec<(&str, &str)> = owned_headers.iter().map(|(name, value)| (*name, value.as_str())).collect();
    let response = match session.request(Method::POST, &url, Some(&body), &headers).await {
        Ok(response) => response,
        Err(error) => return PlaySoundResult::error(error.to_string()),
    };

    if response.status != 200 {
        return PlaySoundResult::error(format!(
            "play sound failed: HTTP {} {}",
            response.status,
            short(&response.body, 300)
        ));
    }
    let data = response.json();
    let success = data.get(api::json::SUCCESS).and_then(Value::as_bool).unwrap_or(true);
    log::info!("[DEVICE] play sound command sent (device={device_id}, success={success})");
    PlaySoundResult { success, message: data.get(api::json::ALERT).and_then(Value::as_str).map(str::to_string) }
}

fn text_pointer(data: &Value, section: &str, field: &str) -> Option<String> {
    data.pointer(&format!("/{section}/{field}")).and_then(Value::as_str).map(str::to_string)
}

fn service_url(data: &Value, service: &str) -> Option<String> {
    let url =
        data.pointer(&format!("/{}/{service}/{}", api::json::WEBSERVICES, api::json::URL)).and_then(Value::as_str)?;
    let trimmed = url.trim_end_matches('/');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn short(body: &str, limit: usize) -> String {
    if body.chars().count() > limit {
        body.chars().take(limit).collect()
    } else {
        body.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_device() -> FmipDevice {
        serde_json::from_value(json!({
            "name": "Peiqi 的 iPhone",
            "modelDisplayName": "iPhone 17 Pro",
            "deviceClass": "iPhone",
            "rawDeviceModel": "iPhone18,3",
            "deviceColor": "1-18-0",
            "batteryLevel": 0.91,
            "batteryStatus": "Charged",
            "locationEnabled": true,
            "id": "device-1",
            "deviceStatus": "200",
            "location": {
                "latitude": 31.23,
                "longitude": 121.47,
                "horizontalAccuracy": 12.5,
                "timeStamp": 1_700_000_000_000i64,
                "isOld": false
            }
        }))
        .expect("样例设备解析失败")
    }

    #[test]
    fn maps_device_and_builds_online_image_url() {
        let device = sample_device();
        let mapped = to_device(None, &device, "https://statici.icloud.com.cn");
        assert_eq!(mapped.name.as_deref(), Some("Peiqi 的 iPhone"));
        assert_eq!(mapped.battery_level, Some(0.91));
        assert_eq!(mapped.device_id.as_deref(), Some("device-1"));
        assert_eq!(mapped.location_enabled, Some(true));
        assert_eq!(
            mapped.device_image_url.as_deref(),
            Some(
                "https://statici.icloud.com.cn/fmipmobile/deviceImages-9.0/iPhone/\
                 iPhone18,3-1-18-0/online-sourcelist.png"
            )
        );
    }

    #[test]
    fn offline_device_uses_offline_variant_and_defaults() {
        let mut device = sample_device();
        device.location = None;
        device.activation_locked = None;
        device.lost_mode_enabled = None;
        device.location_enabled = None;
        let mapped = to_device(Some("https://p12-fmip.icloud.com"), &device, "");
        assert!(mapped.device_image_url.as_deref().unwrap().ends_with("offline-sourcelist.png"));
        assert_eq!(mapped.activation_locked, Some(false));
        assert_eq!(mapped.lost_mode_enabled, Some(false));

        assert_eq!(mapped.location_enabled, Some(true));
        assert_eq!(mapped.lost_mode_pending, Some(false));
    }

    #[test]
    fn missing_model_fields_produce_no_image_url() {
        let mut device = sample_device();
        device.raw_device_model = None;
        assert_eq!(to_device(None, &device, "").device_image_url, None);
    }

    #[test]
    fn service_url_trims_trailing_slash() {
        let data = json!({"webservices": {"findme": {"url": "https://p12-fmip.icloud.com/"}}});
        assert_eq!(service_url(&data, "findme").as_deref(), Some("https://p12-fmip.icloud.com"));
        assert_eq!(service_url(&data, "photos"), None);
    }
}
