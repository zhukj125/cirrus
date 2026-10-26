use std::collections::HashMap;
use std::io::Cursor;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use plist::{Dictionary, Value};

use crate::models::PhotoMetadata;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhotoLocation {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<f64>,
}

pub fn decode_text(encoded: &str) -> Option<String> {
    let bytes = BASE64.decode(encoded.trim()).ok()?;
    let text = String::from_utf8(bytes).ok()?.trim_end_matches('\0').trim().to_string();
    (!text.is_empty()).then_some(text)
}

pub fn parse_keywords(encoded: &str) -> Vec<String> {
    let Some(value) = decode_plist(encoded) else {
        return Vec::new();
    };
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_string)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn parse_location(encoded: &str) -> Option<PhotoLocation> {
    let value = decode_plist(encoded)?;
    let dict = value.as_dictionary()?;
    let latitude = number(dict, "lat")?;
    let longitude = number(dict, "lon")?;
    Some(PhotoLocation { latitude, longitude, altitude: number(dict, "alt") })
}

pub fn parse_media_metadata(encoded: &str) -> Option<PhotoMetadata> {
    let value = decode_plist(encoded)?;
    let root = value.as_dictionary()?;
    if let Some(moop) = root.get("moop").and_then(Value::as_data) {
        return parse_video_metadata(moop);
    }

    let tiff = child_dictionary(root, "{TIFF}");
    let exif = child_dictionary(root, "{Exif}");
    let gps = child_dictionary(root, "{GPS}");
    if tiff.is_none() && exif.is_none() && gps.is_none() {
        return None;
    }

    let latitude = gps.and_then(|value| coordinate(value, "Latitude", "LatitudeRef", "S"));
    let longitude = gps.and_then(|value| coordinate(value, "Longitude", "LongitudeRef", "W"));
    let altitude = gps.and_then(|value| {
        let altitude = number(value, "Altitude")?;
        let below_sea_level = integer(value, "AltitudeRef") == Some(1);
        Some(if below_sea_level { -altitude } else { altitude })
    });
    let metadata = PhotoMetadata {
        camera_make: tiff.and_then(|value| text(value, "Make")),
        camera_model: tiff.and_then(|value| text(value, "Model")),
        lens_model: exif.and_then(|value| text(value, "LensModel")),
        lens_specification: exif.and_then(|value| lens_specification(value, "LensSpecification")),
        date_taken: exif.and_then(|value| text(value, "DateTimeOriginal")),
        exposure_time: exif.and_then(|value| number(value, "ExposureTime")).map(format_exposure),
        f_number: exif.and_then(|value| number(value, "FNumber")).map(|value| format!("f/{}", decimal(value, 1))),
        iso: exif.and_then(|value| first_number(value, "ISOSpeedRatings")).map(|value| decimal(value, 0)),
        focal_length: exif
            .and_then(|value| number(value, "FocalLength"))
            .map(|value| format!("{} mm", decimal(value, 1))),
        focal_length_35mm: exif
            .and_then(|value| number(value, "FocalLenIn35mmFilm"))
            .map(|value| format!("{} mm", decimal(value, 0))),
        exposure_bias: exif
            .and_then(|value| number(value, "ExposureBiasValue"))
            .map(|value| format!("{} EV", decimal(value, 1))),
        metering_mode: exif.and_then(|value| integer(value, "MeteringMode")).and_then(metering_mode),
        flash: exif.and_then(|value| integer(value, "Flash")).map(flash_label),
        white_balance: exif.and_then(|value| integer(value, "WhiteBalance")).and_then(white_balance),
        color_space: exif.and_then(|value| integer(value, "ColorSpace")).and_then(color_space),
        software: tiff.and_then(|value| text(value, "Software")),
        gps_latitude: latitude,
        gps_longitude: longitude,
        gps_altitude: altitude,
    };
    (metadata != PhotoMetadata::default()).then_some(metadata)
}

fn decode_plist(encoded: &str) -> Option<Value> {
    let bytes = BASE64.decode(encoded.trim()).ok()?;
    Value::from_reader(Cursor::new(bytes)).ok()
}

fn child_dictionary<'a>(dict: &'a Dictionary, key: &str) -> Option<&'a Dictionary> {
    dict.get(key)?.as_dictionary()
}

fn text(dict: &Dictionary, key: &str) -> Option<String> {
    let value = dict.get(key)?.as_string()?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn number(dict: &Dictionary, key: &str) -> Option<f64> {
    match dict.get(key)? {
        Value::Real(value) => Some(*value),
        Value::Integer(value) => value.as_signed().map(|value| value as f64),
        _ => None,
    }
}

fn integer(dict: &Dictionary, key: &str) -> Option<i64> {
    match dict.get(key)? {
        Value::Integer(value) => value.as_signed(),
        Value::Real(value) => Some(*value as i64),
        _ => None,
    }
}

fn first_number(dict: &Dictionary, key: &str) -> Option<f64> {
    let value = dict.get(key)?;
    if let Some(numbers) = value.as_array() {
        return numbers.iter().find_map(|value| match value {
            Value::Integer(value) => value.as_signed().map(|value| value as f64),
            Value::Real(value) => Some(*value),
            _ => None,
        });
    }
    number(dict, key)
}

fn coordinate(dict: &Dictionary, value_key: &str, reference_key: &str, negative: &str) -> Option<f64> {
    let value = number(dict, value_key)?;
    let reference = text(dict, reference_key);
    Some(if reference.as_deref() == Some(negative) { -value } else { value })
}

fn lens_specification(dict: &Dictionary, key: &str) -> Option<String> {
    let values: Vec<f64> = dict
        .get(key)?
        .as_array()?
        .iter()
        .filter_map(|value| match value {
            Value::Integer(value) => value.as_signed().map(|value| value as f64),
            Value::Real(value) => Some(*value),
            _ => None,
        })
        .collect();
    if values.len() != 4 {
        return None;
    }
    Some(format!(
        "{}–{} mm · f/{}–{}",
        decimal(values[0], 1),
        decimal(values[1], 1),
        decimal(values[2], 1),
        decimal(values[3], 1)
    ))
}

fn parse_video_metadata(data: &[u8]) -> Option<PhotoMetadata> {
    let fields = quicktime_metadata(data);
    let metadata = PhotoMetadata {
        camera_make: fields.get("com.apple.quicktime.make").cloned(),
        camera_model: fields.get("com.apple.quicktime.model").cloned(),
        software: fields.get("com.apple.quicktime.software").cloned(),
        date_taken: fields.get("com.apple.quicktime.creationdate").cloned(),
        ..PhotoMetadata::default()
    };
    (metadata != PhotoMetadata::default()).then_some(metadata)
}

fn quicktime_metadata(data: &[u8]) -> HashMap<String, String> {
    let Some(keys_position) = find_atom(data, b"keys") else {
        return HashMap::new();
    };
    let Some(ilst_position) = find_atom(data, b"ilst") else {
        return HashMap::new();
    };

    let key_count = read_u32(data, keys_position + 12).unwrap_or(0) as usize;
    let mut keys = Vec::with_capacity(key_count.min(128));
    let mut cursor = keys_position + 16;
    for _ in 0..key_count {
        let Some(size) = read_u32(data, cursor) else {
            break;
        };
        let size = size as usize;
        if size < 8 || cursor + size > data.len() {
            break;
        }
        let key = String::from_utf8_lossy(&data[cursor + 8..cursor + size]).trim_end_matches('\0').to_string();
        keys.push(key);
        cursor += size;
    }

    let ilst_end = ilst_position.saturating_add(read_u32(data, ilst_position).unwrap_or(0) as usize);
    let mut values = HashMap::new();
    cursor = ilst_position + 8;
    while cursor + 16 <= ilst_end.min(data.len()) {
        let Some(entry_size) = read_u32(data, cursor) else {
            break;
        };
        let entry_size = entry_size as usize;
        if entry_size < 16 || cursor + entry_size > data.len() {
            break;
        }
        let key_index = read_u32(data, cursor + 4).unwrap_or(0) as usize;
        let data_start = cursor + 8;
        if data.get(data_start + 4..data_start + 8) == Some(b"data") {
            let payload_start = data_start + 16;
            let data_size = read_u32(data, data_start).unwrap_or(0) as usize;
            if payload_start <= cursor + entry_size && payload_start + data_size.saturating_sub(16) <= data.len() {
                let payload_end = payload_start + data_size.saturating_sub(16);
                let value =
                    String::from_utf8_lossy(&data[payload_start..payload_end]).trim_end_matches('\0').to_string();
                if let Some(key) = key_index.checked_sub(1).and_then(|index| keys.get(index)) {
                    values.insert(key.clone(), value);
                }
            }
        }
        cursor += entry_size;
    }
    values
}

fn find_atom(data: &[u8], atom_type: &[u8; 4]) -> Option<usize> {
    data.windows(4).position(|window| window == atom_type).and_then(|position| {
        let start = position.checked_sub(4)?;
        let size = read_u32(data, start)? as usize;
        (size >= 8 && start + size <= data.len()).then_some(start)
    })
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(u32::from_be_bytes(bytes.try_into().ok()?))
}

fn decimal(value: f64, precision: usize) -> String {
    let mut value = format!("{value:.precision$}");
    while value.contains('.') && value.ends_with('0') {
        value.pop();
    }
    if value.ends_with('.') {
        value.pop();
    }
    value
}

fn format_exposure(value: f64) -> String {
    if value > 0.0 && value < 1.0 {
        return format!("1/{} s", (1.0 / value).round() as i64);
    }
    format!("{} s", decimal(value, 1))
}

fn flash_label(value: i64) -> String {
    match value & 0x1f {
        0x00 => "No flash".to_string(),
        0x01 => "Fired".to_string(),
        0x10 => "No flash function".to_string(),
        _ => format!("Flash mode {value}"),
    }
}

fn white_balance(value: i64) -> Option<String> {
    Some(match value {
        0 => "Auto".to_string(),
        1 => "Manual".to_string(),
        _ => format!("Mode {value}"),
    })
}

fn metering_mode(value: i64) -> Option<String> {
    Some(match value {
        0 => "Unknown".to_string(),
        1 => "Average".to_string(),
        2 => "Center-weighted average".to_string(),
        3 => "Spot".to_string(),
        4 => "Multi-spot".to_string(),
        5 => "Pattern".to_string(),
        6 => "Partial".to_string(),
        _ => format!("Mode {value}"),
    })
}

fn color_space(value: i64) -> Option<String> {
    Some(match value {
        1 => "sRGB".to_string(),
        2 => "Adobe RGB".to_string(),
        65535 => "Uncalibrated".to_string(),
        _ => format!("Color space {value}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(value: Value) -> String {
        let mut bytes = Vec::new();
        plist::to_writer_binary(&mut bytes, &value).expect("binary plist");
        BASE64.encode(bytes)
    }

    fn dict(entries: &[(&str, Value)]) -> Dictionary {
        entries.iter().map(|(key, value)| ((*key).to_string(), value.clone())).collect()
    }

    #[test]
    fn parses_camera_metadata_from_cloudkit_plist() {
        let tiff = Value::Dictionary(dict(&[
            ("Make", Value::String("Apple".to_string())),
            ("Model", Value::String("iPhone 13".to_string())),
            ("Software", Value::String("16.6.1".to_string())),
        ]));
        let exif = Value::Dictionary(dict(&[
            ("FNumber", Value::Real(1.6)),
            ("ExposureTime", Value::Real(0.02)),
            ("ISOSpeedRatings", Value::Array(vec![Value::Integer(500.into())])),
            ("FocalLength", Value::Real(5.1)),
            ("FocalLenIn35mmFilm", Value::Integer(26.into())),
            ("MeteringMode", Value::Integer(5.into())),
        ]));
        let root = Value::Dictionary(dict(&[("{TIFF}", tiff), ("{Exif}", exif)]));

        let metadata = parse_media_metadata(&encode(root)).expect("metadata");
        assert_eq!(metadata.camera_make.as_deref(), Some("Apple"));
        assert_eq!(metadata.camera_model.as_deref(), Some("iPhone 13"));
        assert_eq!(metadata.f_number.as_deref(), Some("f/1.6"));
        assert_eq!(metadata.exposure_time.as_deref(), Some("1/50 s"));
        assert_eq!(metadata.iso.as_deref(), Some("500"));
        assert_eq!(metadata.focal_length.as_deref(), Some("5.1 mm"));
        assert_eq!(metadata.metering_mode.as_deref(), Some("Pattern"));
    }

    fn atom(kind: &[u8; 4], payload: Vec<u8>) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(payload.len() + 8);
        bytes.extend(((payload.len() + 8) as u32).to_be_bytes());
        bytes.extend(kind);
        bytes.extend(payload);
        bytes
    }

    fn quicktime_bytes(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut keys = Vec::new();
        keys.extend(0u32.to_be_bytes());
        keys.extend((entries.len() as u32).to_be_bytes());
        for (key, _) in entries {
            keys.extend(((key.len() + 8) as u32).to_be_bytes());
            keys.extend(b"mdta");
            keys.extend(key.as_bytes());
        }
        let mut list = Vec::new();
        for (index, (_, value)) in entries.iter().enumerate() {
            let mut data = Vec::new();
            data.extend(1u32.to_be_bytes());
            data.extend(0u32.to_be_bytes());
            data.extend(value.as_bytes());
            let data = atom(b"data", data);
            let mut item = Vec::new();
            item.extend(((data.len() + 8) as u32).to_be_bytes());
            item.extend(((index + 1) as u32).to_be_bytes());
            item.extend(data);
            list.extend(item);
        }
        atom(b"moov", [atom(b"keys", keys), atom(b"ilst", list)].concat())
    }

    #[test]
    fn parses_video_metadata_from_quicktime_atoms() {
        let moop = quicktime_bytes(&[
            ("com.apple.quicktime.make", "Apple"),
            ("com.apple.quicktime.model", "iPhone 13"),
            ("com.apple.quicktime.software", "16.6.1"),
        ]);
        let root = Value::Dictionary(dict(&[("moop", Value::Data(moop))]));
        let metadata = parse_media_metadata(&encode(root)).expect("video metadata");
        assert_eq!(metadata.camera_make.as_deref(), Some("Apple"));
        assert_eq!(metadata.camera_model.as_deref(), Some("iPhone 13"));
        assert_eq!(metadata.software.as_deref(), Some("16.6.1"));
    }

    #[test]
    fn parses_location_plist() {
        let value = Value::Dictionary(dict(&[
            ("lat", Value::Real(31.2304)),
            ("lon", Value::Real(121.4737)),
            ("alt", Value::Real(8.0)),
        ]));
        let location = parse_location(&encode(value)).expect("location");
        assert_eq!(location.latitude, 31.2304);
        assert_eq!(location.longitude, 121.4737);
        assert_eq!(location.altitude, Some(8.0));
    }

    #[test]
    fn decodes_cloudkit_file_names() {
        let encoded = BASE64.encode("IMG_3523.MOV");
        assert_eq!(decode_text(&encoded).as_deref(), Some("IMG_3523.MOV"));
    }
}
