use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

const NOMINATIM_MIN_INTERVAL: Duration = Duration::from_millis(1_100);
static NOMINATIM_GATE: tokio::sync::Mutex<Option<Instant>> = tokio::sync::Mutex::const_new(None);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReverseGeocodeRequest {
    pub longitude: f64,
    pub latitude: f64,
    pub wgs84_longitude: f64,
    pub wgs84_latitude: f64,
    pub language: String,
    pub amap_key: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReverseGeocodeResult {
    pub full: String,
    pub short: String,
}

fn text(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).map(str::trim).unwrap_or_default().to_string()
}

fn pick(address: &Value, keys: &[&str]) -> String {
    keys.iter().map(|key| text(address.get(*key))).find(|value| !value.is_empty()).unwrap_or_default()
}

fn join_parts(parts: &[String], language: &str) -> String {
    parts.iter().filter(|part| !part.is_empty()).cloned().collect::<Vec<_>>().join(if language == "en" {
        ", "
    } else {
        "，"
    })
}

fn build_amap_result(value: &Value, language: &str) -> Option<ReverseGeocodeResult> {
    if text(value.get("status")) != "1" {
        return None;
    }
    let regeocode = value.get("regeocode")?;
    let address = regeocode.get("addressComponent").unwrap_or(&Value::Null);
    let street_number = address.get("streetNumber").unwrap_or(&Value::Null);
    let street = text(street_number.get("street"));
    let number = text(street_number.get("number"));
    let city = pick(address, &["city", "province"]);
    let short = join_parts(
        &[if street.is_empty() { pick(address, &["township", "district"]) } else { street.clone() }, city],
        language,
    );
    let fallback = join_parts(
        &[
            text(address.get("province")),
            text(address.get("city")),
            text(address.get("district")),
            text(address.get("township")),
            street,
            number,
        ],
        language,
    );
    let full = text(regeocode.get("formatted_address")).trim_start_matches("中国").trim().to_string();
    let full = if full.is_empty() { fallback } else { full };
    (!full.is_empty()).then_some(ReverseGeocodeResult { full, short })
}

fn build_nominatim_result(value: &Value, language: &str) -> Option<ReverseGeocodeResult> {
    let address = value.get("address").unwrap_or(&Value::Null);
    let street = pick(address, &["road", "pedestrian", "footway", "residential", "suburb", "neighbourhood", "quarter"]);
    let locality = pick(address, &["city", "town", "county", "district", "state"]);
    let short = join_parts(&[street, locality], language);
    let full =
        text(value.get("display_name")).trim_end_matches(", 中国").trim_end_matches(", China").trim().to_string();
    (!full.is_empty()).then_some(ReverseGeocodeResult { full, short })
}

async fn fetch_amap(
    client: &reqwest::Client,
    payload: &ReverseGeocodeRequest,
    language: &str,
) -> Option<ReverseGeocodeResult> {
    let key = payload.amap_key.as_deref()?.trim();
    if key.is_empty() || key.len() > 256 {
        return None;
    }
    let lang_code = if language == "en" { "en" } else { "zh-Hans" };
    let response = client
        .get("https://restapi.amap.com/v3/geocode/regeo")
        .query(&[
            ("key", key),
            ("location", &format!("{},{}", payload.longitude, payload.latitude)),
            ("extensions", "base"),
            ("output", "json"),
            ("lang", language),
            ("langCode", lang_code),
        ])
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    build_amap_result(&response.json::<Value>().await.ok()?, language)
}

async fn wait_for_nominatim_slot() {
    let mut last_request = NOMINATIM_GATE.lock().await;
    if let Some(last) = *last_request {
        let elapsed = last.elapsed();
        if elapsed < NOMINATIM_MIN_INTERVAL {
            tokio::time::sleep(NOMINATIM_MIN_INTERVAL - elapsed).await;
        }
    }
    *last_request = Some(Instant::now());
}

async fn fetch_nominatim(
    client: &reqwest::Client,
    payload: &ReverseGeocodeRequest,
    language: &str,
) -> Option<ReverseGeocodeResult> {
    wait_for_nominatim_slot().await;
    let accept_language = if language == "en" { "en" } else { "zh-CN" };
    let response = client
        .get("https://nominatim.openstreetmap.org/reverse")
        .query(&[
            ("format", "jsonv2"),
            ("lat", &payload.wgs84_latitude.to_string()),
            ("lon", &payload.wgs84_longitude.to_string()),
            ("zoom", "18"),
            ("accept-language", accept_language),
        ])
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    build_nominatim_result(&response.json::<Value>().await.ok()?, language)
}

#[tauri::command]
pub async fn reverse_geocode(payload: ReverseGeocodeRequest) -> Result<Option<ReverseGeocodeResult>, String> {
    if !payload.longitude.is_finite()
        || !payload.latitude.is_finite()
        || !(-180.0..=180.0).contains(&payload.longitude)
        || !(-90.0..=90.0).contains(&payload.latitude)
        || !payload.wgs84_longitude.is_finite()
        || !payload.wgs84_latitude.is_finite()
        || !(-180.0..=180.0).contains(&payload.wgs84_longitude)
        || !(-90.0..=90.0).contains(&payload.wgs84_latitude)
    {
        return Err("Invalid reverse geocoding coordinates".to_string());
    }
    let language = if payload.language == "en" { "en" } else { "zh" };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent(format!("Cirrus/{} (https://github.com/zhukj125/cirrus)", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| format!("Could not create reverse geocoding client: {error}"))?;

    if let Some(result) = fetch_amap(&client, &payload, language).await {
        return Ok(Some(result));
    }
    log::warn!("[GEO] AMap reverse geocoding failed; falling back to OpenStreetMap");
    Ok(fetch_nominatim(&client, &payload, language).await)
}

#[cfg(test)]
mod tests {
    use super::{build_amap_result, build_nominatim_result};
    use serde_json::json;

    #[test]
    fn parses_amap_regeocode() {
        let value = json!({
            "status": "1",
            "regeocode": {
                "formatted_address": "中国浙江省杭州市西湖区测试路1号",
                "addressComponent": {
                    "province": "浙江省",
                    "city": "杭州市",
                    "district": "西湖区",
                    "streetNumber": { "street": "测试路", "number": "1号" }
                }
            }
        });
        let result = build_amap_result(&value, "zh").expect("address");
        assert_eq!(result.full, "浙江省杭州市西湖区测试路1号");
        assert_eq!(result.short, "测试路，杭州市");
    }

    #[test]
    fn parses_nominatim_regeocode() {
        let value = json!({
            "display_name": "Test Road, Xihu, Hangzhou, Zhejiang, China",
            "address": {
                "road": "Test Road",
                "city": "Hangzhou",
                "state": "Zhejiang",
                "country": "China"
            }
        });
        let result = build_nominatim_result(&value, "en").expect("address");
        assert_eq!(result.full, "Test Road, Xihu, Hangzhou, Zhejiang");
        assert_eq!(result.short, "Test Road, Hangzhou");
    }
}
