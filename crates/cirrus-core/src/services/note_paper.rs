use std::collections::HashMap;
use std::fmt::Write;
use std::io::Read;

use flate2::read::DeflateDecoder;
use rusqlite::{Connection, OpenFlags};

use crate::error::{AppError, AppResult};

const MAX_DATABASE_BYTES: usize = 64 * 1024 * 1024;
const MAX_STROKES: usize = 20_000;

#[derive(Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
    width: f64,
}

#[derive(Clone)]
struct Stroke {
    points: Vec<Point>,
    color: [f32; 4],
    marker: bool,
    created: f64,
}

struct ReferenceRow {
    id: Vec<u8>,
    data: Vec<u8>,
    children: Vec<Vec<u8>>,
}

#[derive(Clone, Copy)]
enum Wire<'a> {
    Varint(u64),
    Fixed64(&'a [u8]),
    Bytes(&'a [u8]),
    Fixed32(&'a [u8]),
}

pub fn render_svg(database: &[u8], width: f64, height: f64) -> AppResult<String> {
    let sqlite = extract_sqlite(database)?;
    if sqlite.len() > MAX_DATABASE_BYTES {
        return Err(AppError::other("Paper database is too large to render"));
    }

    let path = std::env::temp_dir().join(format!("cirrus-paper-{}.sqlite3", uuid::Uuid::new_v4()));
    std::fs::write(&path, &sqlite)?;
    let result = render_sqlite(&path, width, height);
    let _ = std::fs::remove_file(path);
    result
}

fn render_sqlite(path: &std::path::Path, width: f64, height: f64) -> AppResult<String> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut statement = connection.prepare("SELECT Id, Data, ChildRetainCounts FROM Reference")?;
    let mapped = statement
        .query_map([], |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?, row.get::<_, Vec<u8>>(2)?)))?;

    let mut rows = Vec::new();
    for row in mapped {
        let (id, data, child_bytes) = row?;
        let children = child_ids(&child_bytes);
        rows.push(ReferenceRow { id, data, children });
    }
    render_rows(&rows, width, height)
}

fn child_ids(data: &[u8]) -> Vec<Vec<u8>> {
    let Ok(fields) = parse_proto(data) else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    for (field, _wire, value) in fields {
        if field != 2 {
            continue;
        }
        let Wire::Bytes(entry) = value else {
            continue;
        };
        let Ok(entry_fields) = parse_proto(entry) else {
            continue;
        };
        if let Some(Wire::Bytes(id)) = first_field(&entry_fields, 2) {
            if !id.is_empty() {
                ids.push(id.to_vec());
            }
        }
    }
    ids
}

fn render_rows(rows: &[ReferenceRow], width: f64, height: f64) -> AppResult<String> {
    if rows.is_empty() {
        return Err(AppError::other("Paper database has no drawing references"));
    }

    let by_id: HashMap<&[u8], &ReferenceRow> = rows.iter().map(|row| (row.id.as_slice(), row)).collect();
    let mut parents: HashMap<&[u8], Vec<&[u8]>> = HashMap::new();
    for row in rows {
        for child in &row.children {
            parents.entry(child.as_slice()).or_default().push(row.id.as_slice());
        }
    }

    let mut strokes = Vec::new();
    for row in rows {
        let Some(payload) = stroke_payload(&row.data) else {
            continue;
        };
        let Some(style) = find_style(&by_id, &parents, row) else {
            continue;
        };
        if let Some(stroke) = payload_to_stroke(payload, style) {
            strokes.push(stroke);
        }
        if strokes.len() >= MAX_STROKES {
            break;
        }
    }
    if strokes.is_empty() {
        return Err(AppError::other("Paper database contains no supported strokes"));
    }

    strokes.sort_by(|left, right| {
        left.marker.cmp(&right.marker).reverse().then_with(|| left.created.total_cmp(&right.created))
    });

    let (canvas_width, canvas_height) = canvas_size(&strokes, width, height);
    let intrinsic_width = canvas_width * 4.0;
    let intrinsic_height = canvas_height * 4.0;
    let mut svg = String::new();
    write!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" preserveAspectRatio=\"xMidYMid meet\">",
        number(intrinsic_width),
        number(intrinsic_height),
        number(canvas_width),
        number(canvas_height)
    )
    .expect("writing to string cannot fail");
    write!(svg, "<rect width=\"100%\" height=\"100%\" fill=\"#fff\"/>").expect("writing to string cannot fail");

    for stroke in &strokes {
        let [red, green, blue, alpha] = stroke.color;
        let color = format!("rgb({:.1},{:.1},{:.1})", red * 255.0, green * 255.0, blue * 255.0);
        if stroke.marker {
            write!(
                svg,
                "<path d=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-opacity=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
                smooth_path(&stroke.points),
                number(alpha as f64),
                number(stroke.points.first().map_or(13.75, |point| point.width))
            )
            .expect("writing to string cannot fail");
        } else {
            for pair in stroke.points.windows(2) {
                let (start, end) = (pair[0], pair[1]);
                let width = (start.width + end.width) / 2.0;
                write!(
                    svg,
                    "<path d=\"M {} {} L {} {}\" fill=\"none\" stroke=\"{color}\" stroke-opacity=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
                    number(start.x),
                    number(start.y),
                    number(end.x),
                    number(end.y),
                    number(alpha as f64),
                    number(width)
                )
                .expect("writing to string cannot fail");
            }
        }
    }
    svg.push_str("</svg>");
    Ok(svg)
}

fn find_style<'a>(
    by_id: &HashMap<&'a [u8], &'a ReferenceRow>,
    parents: &HashMap<&'a [u8], Vec<&'a [u8]>>,
    stroke: &'a ReferenceRow,
) -> Option<(bool, [f32; 4])> {
    for parent_id in parents.get(stroke.id.as_slice()).into_iter().flatten() {
        let Some(parent) = by_id.get(parent_id) else {
            continue;
        };
        for sibling_id in &parent.children {
            if sibling_id == &stroke.id {
                continue;
            }
            let Some(sibling) = by_id.get(sibling_id.as_slice()) else {
                continue;
            };
            if let Some((name, color)) = parse_style(&sibling.data) {
                return Some((name.ends_with("marker") || name.ends_with("highlighter"), color));
            }
        }
    }
    None
}

fn parse_style(data: &[u8]) -> Option<(String, [f32; 4])> {
    parse_style_inner(data, 0)
}

fn parse_style_inner(data: &[u8], depth: usize) -> Option<(String, [f32; 4])> {
    if depth > 12 {
        return None;
    }
    let fields = parse_proto(data).ok()?;
    let name = match first_field(&fields, 2) {
        Some(Wire::Bytes(bytes)) if bytes.starts_with(b"com.apple.ink.") => String::from_utf8_lossy(bytes).to_string(),
        _ => String::new(),
    };
    if !name.is_empty() {
        if let Some(Wire::Bytes(raw_color)) = first_field(&fields, 1) {
            if let Ok(color_fields) = parse_proto(raw_color) {
                let mut channels = Vec::new();
                for field in 1..=4 {
                    if let Some(Wire::Fixed32(bytes)) = first_field(&color_fields, field) {
                        channels.push(f32::from_le_bytes(bytes.try_into().ok()?));
                    }
                }
                if channels.len() == 4 {
                    return Some((name, [channels[0], channels[1], channels[2], channels[3]]));
                }
            }
        }
    }
    for (_, _wire, value) in fields {
        if let Wire::Bytes(bytes) = value {
            if let Some(style) = parse_style_inner(bytes, depth + 1) {
                return Some(style);
            }
        }
    }
    None
}

struct StrokePayload<'a> {
    tool: u64,
    packed: &'a [u8],
    default_width: Option<f32>,
    created: f64,
}

fn stroke_payload(data: &[u8]) -> Option<StrokePayload<'_>> {
    let version = bytes_field(data, 1)?;
    let drawing = bytes_field(version, 10)?;
    let stroke = bytes_field(drawing, 1)?;
    let payload = bytes_field(stroke, 2)?;
    let tool = varint_field(payload, 4)?;
    let packed = bytes_field(payload, 7)?;
    let default_width = bytes_field(payload, 6).and_then(|bytes| {
        if bytes.len() < 4 {
            return None;
        }
        let value = f32::from_le_bytes(bytes[0..4].try_into().ok()?);
        (value.is_finite() && value > 0.0).then_some(value)
    });
    let created =
        fixed64_field(payload, 2).and_then(|bytes| bytes.try_into().ok()).map(f64::from_le_bytes).unwrap_or(0.0);
    Some(StrokePayload { tool, packed, default_width, created })
}

fn bytes_field<'a>(data: &'a [u8], field: u64) -> Option<&'a [u8]> {
    let fields = parse_proto(data).ok()?;
    fields.into_iter().find_map(|(candidate, _, value)| match value {
        Wire::Bytes(bytes) if candidate == field => Some(bytes),
        _ => None,
    })
}

fn varint_field(data: &[u8], field: u64) -> Option<u64> {
    let fields = parse_proto(data).ok()?;
    fields.into_iter().find_map(|(candidate, _, value)| match value {
        Wire::Varint(number) if candidate == field => Some(number),
        _ => None,
    })
}

fn fixed64_field<'a>(data: &'a [u8], field: u64) -> Option<&'a [u8]> {
    let fields = parse_proto(data).ok()?;
    fields.into_iter().find_map(|(candidate, _, value)| match value {
        Wire::Fixed64(bytes) if candidate == field => Some(bytes),
        _ => None,
    })
}
fn payload_to_stroke(payload: StrokePayload<'_>, (marker, color): (bool, [f32; 4])) -> Option<Stroke> {
    let stride = match payload.tool {
        7 => 16,
        3 => 12,
        259 => 14,
        _ if marker => 14,
        _ => 16,
    };
    if payload.packed.is_empty() || payload.packed.len() % stride != 0 {
        return None;
    }
    let fixed_width = payload.default_width.unwrap_or(if marker { 13.75 } else { 3.2 }) as f64;
    let mut points = Vec::new();
    for chunk in payload.packed.chunks_exact(stride) {
        let x = f32::from_le_bytes(chunk[0..4].try_into().ok()?) as f64;
        let y = f32::from_le_bytes(chunk[4..8].try_into().ok()?) as f64;
        if !x.is_finite() || !y.is_finite() {
            continue;
        }
        let width = if stride == 16 { f32::from_le_bytes(chunk[12..16].try_into().ok()?) as f64 } else { fixed_width };
        if !width.is_finite() || width <= 0.0 {
            continue;
        }
        points.push(Point { x, y, width });
    }
    if points.is_empty() {
        return None;
    }
    Some(Stroke { points, color, marker, created: payload.created })
}

fn canvas_size(strokes: &[Stroke], width: f64, height: f64) -> (f64, f64) {
    let max_x = strokes.iter().flat_map(|stroke| stroke.points.iter()).map(|point| point.x).fold(0.0_f64, f64::max);
    let max_y = strokes.iter().flat_map(|stroke| stroke.points.iter()).map(|point| point.y).fold(0.0_f64, f64::max);
    let width = if width.is_finite() && width > 0.0 { width } else { (max_x + 24.0).max(320.0) };
    let height = if height.is_finite() && height > 0.0 { height } else { (max_y + 24.0).max(200.0) };
    (width, height)
}

fn smooth_path(points: &[Point]) -> String {
    if points.is_empty() {
        return String::new();
    }
    let mut path = format!("M {} {}", number(points[0].x), number(points[0].y));
    if points.len() == 1 {
        return path;
    }
    if points.len() == 2 {
        write!(path, " L {} {}", number(points[1].x), number(points[1].y)).expect("writing to string cannot fail");
        return path;
    }
    for index in 0..points.len() - 1 {
        let previous = points[index.saturating_sub(1)];
        let current = points[index];
        let next = points[index + 1];
        let following = points[(index + 2).min(points.len() - 1)];
        let control_one = (current.x + (next.x - previous.x) / 6.0, current.y + (next.y - previous.y) / 6.0);
        let control_two = (next.x - (following.x - current.x) / 6.0, next.y - (following.y - current.y) / 6.0);
        write!(
            path,
            " C {} {} {} {} {} {}",
            number(control_one.0),
            number(control_one.1),
            number(control_two.0),
            number(control_two.1),
            number(next.x),
            number(next.y)
        )
        .expect("writing to string cannot fail");
    }
    path
}

fn number(value: f64) -> String {
    if !value.is_finite() || value.abs() < 0.0005 {
        return "0".to_string();
    }
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn extract_sqlite(zip: &[u8]) -> AppResult<Vec<u8>> {
    if !zip.starts_with(b"PK\x03\x04") {
        return Err(AppError::other("Paper database is not a zip archive"));
    }
    let eocd = zip
        .windows(4)
        .rposition(|window| window == b"PK\x05\x06")
        .ok_or_else(|| AppError::other("Paper database zip has no central directory"))?;
    let entry_count = read_u16(zip, eocd + 10)? as usize;
    let central_offset = read_u32(zip, eocd + 16)? as usize;
    let mut position = central_offset;

    for _ in 0..entry_count {
        if read_u32(zip, position)? != 0x0201_4b50 {
            return Err(AppError::other("Paper database zip has an invalid central directory entry"));
        }
        let method = read_u16(zip, position + 10)?;
        let compressed_size = read_u32(zip, position + 20)? as usize;
        let name_length = read_u16(zip, position + 28)? as usize;
        let extra_length = read_u16(zip, position + 30)? as usize;
        let comment_length = read_u16(zip, position + 32)? as usize;
        let local_offset = read_u32(zip, position + 42)? as usize;
        let name = zip
            .get(position + 46..position + 46 + name_length)
            .ok_or_else(|| AppError::other("Paper database zip has a truncated file name"))?;
        let normalized = String::from_utf8_lossy(name).trim_start_matches('/').to_string();
        if normalized == "data.sqlite3" || normalized.ends_with("/data.sqlite3") {
            if read_u32(zip, local_offset)? != 0x0403_4b50 {
                return Err(AppError::other("Paper database zip has an invalid local header"));
            }
            let local_name_length = read_u16(zip, local_offset + 26)? as usize;
            let local_extra_length = read_u16(zip, local_offset + 28)? as usize;
            let data_start = local_offset + 30 + local_name_length + local_extra_length;
            let data_end = data_start
                .checked_add(compressed_size)
                .filter(|end| *end <= zip.len())
                .ok_or_else(|| AppError::other("Paper database zip has truncated data"))?;
            let compressed = &zip[data_start..data_end];
            return match method {
                0 => Ok(compressed.to_vec()),
                8 => {
                    let mut output = Vec::new();
                    DeflateDecoder::new(compressed).read_to_end(&mut output)?;
                    Ok(output)
                }
                _ => Err(AppError::other(format!("Unsupported paper database zip compression: {method}"))),
            };
        }
        position += 46 + name_length + extra_length + comment_length;
    }
    Err(AppError::other("Paper database zip does not contain data.sqlite3"))
}

fn read_u16(data: &[u8], offset: usize) -> AppResult<u16> {
    let bytes =
        data.get(offset..offset + 2).ok_or_else(|| AppError::other("Paper database contains a truncated integer"))?;
    Ok(u16::from_le_bytes(bytes.try_into().map_err(|_| AppError::other("Invalid u16"))?))
}

fn read_u32(data: &[u8], offset: usize) -> AppResult<u32> {
    let bytes =
        data.get(offset..offset + 4).ok_or_else(|| AppError::other("Paper database contains a truncated integer"))?;
    Ok(u32::from_le_bytes(bytes.try_into().map_err(|_| AppError::other("Invalid u32"))?))
}

fn parse_proto(data: &[u8]) -> AppResult<Vec<(u64, u8, Wire<'_>)>> {
    let mut fields = Vec::new();
    let mut position = 0;
    while position < data.len() {
        let tag = proto_varint(data, &mut position)?;
        let field = tag >> 3;
        let wire = (tag & 7) as u8;
        if field == 0 {
            return Err(AppError::other("Protobuf contains field zero"));
        }
        let value = match wire {
            0 => Wire::Varint(proto_varint(data, &mut position)?),
            1 => {
                let bytes =
                    data.get(position..position + 8).ok_or_else(|| AppError::other("Protobuf fixed64 is truncated"))?;
                position += 8;
                Wire::Fixed64(bytes)
            }
            2 => {
                let length = proto_varint(data, &mut position)? as usize;
                let end = position
                    .checked_add(length)
                    .filter(|end| *end <= data.len())
                    .ok_or_else(|| AppError::other("Protobuf bytes are truncated"))?;
                let bytes = &data[position..end];
                position = end;
                Wire::Bytes(bytes)
            }
            5 => {
                let bytes =
                    data.get(position..position + 4).ok_or_else(|| AppError::other("Protobuf fixed32 is truncated"))?;
                position += 4;
                Wire::Fixed32(bytes)
            }
            other => return Err(AppError::other(format!("Unsupported protobuf wire type: {other}"))),
        };
        fields.push((field, wire, value));
    }
    Ok(fields)
}

fn proto_varint(data: &[u8], position: &mut usize) -> AppResult<u64> {
    let mut value = 0_u64;
    for shift in (0..70).step_by(7) {
        let byte = *data.get(*position).ok_or_else(|| AppError::other("Protobuf varint is truncated"))?;
        *position += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(AppError::other("Protobuf varint is too long"))
}

fn first_field<'a>(fields: &'a [(u64, u8, Wire<'a>)], field: u64) -> Option<Wire<'a>> {
    fields.iter().find_map(|(candidate, _, value)| (*candidate == field).then_some(*value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_stored_sqlite_from_zip() {
        let payload = b"SQLite format 3\0test";
        let mut zip = Vec::new();
        zip.extend_from_slice(b"PK\x03\x04");
        zip.extend_from_slice(&20_u16.to_le_bytes());
        zip.extend_from_slice(&0_u16.to_le_bytes());
        zip.extend_from_slice(&0_u16.to_le_bytes());
        zip.extend_from_slice(&0_u16.to_le_bytes());
        zip.extend_from_slice(&0_u16.to_le_bytes());
        zip.extend_from_slice(&0_u32.to_le_bytes());
        zip.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        zip.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        zip.extend_from_slice(&(b"data.sqlite3".len() as u16).to_le_bytes());
        zip.extend_from_slice(&0_u16.to_le_bytes());
        zip.extend_from_slice(b"data.sqlite3");
        zip.extend_from_slice(payload);
        let central_offset = zip.len() as u32;
        zip.extend_from_slice(b"PK\x01\x02");
        zip.extend_from_slice(&20_u16.to_le_bytes());
        zip.extend_from_slice(&20_u16.to_le_bytes());
        zip.extend_from_slice(&0_u16.to_le_bytes());
        zip.extend_from_slice(&0_u16.to_le_bytes());
        zip.extend_from_slice(&0_u16.to_le_bytes());
        zip.extend_from_slice(&0_u16.to_le_bytes());
        zip.extend_from_slice(&0_u32.to_le_bytes());
        zip.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        zip.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        zip.extend_from_slice(&(b"data.sqlite3".len() as u16).to_le_bytes());
        zip.extend_from_slice(&0_u16.to_le_bytes());
        zip.extend_from_slice(&0_u16.to_le_bytes());
        zip.extend_from_slice(&0_u16.to_le_bytes());
        zip.extend_from_slice(&0_u16.to_le_bytes());
        zip.extend_from_slice(&0_u32.to_le_bytes());
        zip.extend_from_slice(&0_u32.to_le_bytes());
        zip.extend_from_slice(b"data.sqlite3");
        let central_size = zip.len() as u32 - central_offset;
        zip.extend_from_slice(b"PK\x05\x06");
        zip.extend_from_slice(&0_u16.to_le_bytes());
        zip.extend_from_slice(&0_u16.to_le_bytes());
        zip.extend_from_slice(&1_u16.to_le_bytes());
        zip.extend_from_slice(&1_u16.to_le_bytes());
        zip.extend_from_slice(&central_size.to_le_bytes());
        zip.extend_from_slice(&central_offset.to_le_bytes());
        zip.extend_from_slice(&0_u16.to_le_bytes());
        assert_eq!(extract_sqlite(&zip).expect("sqlite"), payload);
    }

    #[test]
    fn formats_vector_numbers_compactly() {
        assert_eq!(number(12.5), "12.5");
        assert_eq!(number(-0.0001), "0");
    }
}
