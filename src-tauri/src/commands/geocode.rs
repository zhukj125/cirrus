use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReverseGeocodeRequest {
    pub longitude: f64,
    pub latitude: f64,
    pub language: String,
    pub amap_key: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReverseGeocodeResult {
    pub full: String,
    pub short: String,
}

const GCJ_A: f64 = 6_378_245.0;
const GCJ_EE: f64 = 0.006_693_421_622_965_943;

fn outside_china(longitude: f64, latitude: f64) -> bool {
    longitude < 72.004 || longitude > 137.8347 || latitude < 0.8293 || latitude > 55.8271
}

fn transform_latitude(x: f64, y: f64) -> f64 {
    let mut value = -100.0 + 2.0 * x + 3.0 * y + 0.2 * y * y + 0.1 * x * y + 0.2 * x.abs().sqrt();
    value +=
        ((20.0 * (6.0 * x * std::f64::consts::PI).sin() + 20.0 * (2.0 * x * std::f64::consts::PI).sin()) * 2.0) / 3.0;
    value += ((20.0 * (y * std::f64::consts::PI).sin() + 40.0 * ((y / 3.0) * std::f64::consts::PI).sin()) * 2.0) / 3.0;
    value += ((160.0 * ((y / 12.0) * std::f64::consts::PI).sin() + 320.0 * ((y * std::f64::consts::PI) / 30.0).sin())
        * 2.0)
        / 3.0;
    value
}

fn transform_longitude(x: f64, y: f64) -> f64 {
    let mut value = 300.0 + x + 2.0 * y + 0.1 * x * x + 0.1 * x * y + 0.1 * x.abs().sqrt();
    value +=
        ((20.0 * (6.0 * x * std::f64::consts::PI).sin() + 20.0 * (2.0 * x * std::f64::consts::PI).sin()) * 2.0) / 3.0;
    value += ((20.0 * (x * std::f64::consts::PI).sin() + 40.0 * ((x / 3.0) * std::f64::consts::PI).sin()) * 2.0) / 3.0;
    value += ((150.0 * ((x / 12.0) * std::f64::consts::PI).sin() + 300.0 * ((x / 30.0) * std::f64::consts::PI).sin())
        * 2.0)
        / 3.0;
    value
}

pub(crate) fn wgs84_to_gcj02(longitude: f64, latitude: f64) -> (f64, f64) {
    if outside_china(longitude, latitude) {
        return (longitude, latitude);
    }
    let mut latitude_delta = transform_latitude(longitude - 105.0, latitude - 35.0);
    let mut longitude_delta = transform_longitude(longitude - 105.0, latitude - 35.0);
    let radian_latitude = latitude / 180.0 * std::f64::consts::PI;
    let magic = 1.0 - GCJ_EE * radian_latitude.sin().powi(2);
    let sqrt_magic = magic.sqrt();
    latitude_delta =
        latitude_delta * 180.0 / (((GCJ_A * (1.0 - GCJ_EE)) / (magic * sqrt_magic)) * std::f64::consts::PI);
    longitude_delta = longitude_delta * 180.0 / ((GCJ_A / sqrt_magic) * radian_latitude.cos() * std::f64::consts::PI);
    (longitude + longitude_delta, latitude + latitude_delta)
}

pub(crate) async fn reverse_geocode_wgs84(
    longitude: f64,
    latitude: f64,
    amap_key: Option<String>,
    language: &str,
) -> Option<ReverseGeocodeResult> {
    let (longitude, latitude) = wgs84_to_gcj02(longitude, latitude);
    reverse_geocode(ReverseGeocodeRequest {
        longitude,
        latitude,
        language: if language == "en" { "en" } else { "zh" }.to_string(),
        amap_key,
    })
    .await
    .ok()
    .flatten()
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

#[tauri::command]
pub async fn reverse_geocode(payload: ReverseGeocodeRequest) -> Result<Option<ReverseGeocodeResult>, String> {
    if !payload.longitude.is_finite()
        || !payload.latitude.is_finite()
        || !(-180.0..=180.0).contains(&payload.longitude)
        || !(-90.0..=90.0).contains(&payload.latitude)
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
    log::warn!("[GEO] AMap reverse geocoding returned no address");
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::{build_amap_result, wgs84_to_gcj02};
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
    fn converts_wgs84_to_gcj02_only_inside_china() {
        assert_eq!(wgs84_to_gcj02(-122.4194, 37.7749), (-122.4194, 37.7749));
        let (longitude, latitude) = wgs84_to_gcj02(120.10530223755852, 30.28579257472865);
        assert!((longitude - 120.10530223755852).abs() > 0.001);
        assert!((latitude - 30.28579257472865).abs() > 0.001);
        assert!((longitude - 120.10530223755852).abs() < 0.02);
        assert!((latitude - 30.28579257472865).abs() < 0.02);
    }
}
