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
    use super::build_amap_result;
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
}
