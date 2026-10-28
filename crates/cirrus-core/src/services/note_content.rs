use std::collections::HashMap;
use std::fmt::Write;
use std::io::Read;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use flate2::read::{DeflateDecoder, GzDecoder, ZlibDecoder};

use crate::error::{AppError, AppResult};
use crate::models::NoteAttachment;

const MAX_DECODED_BYTES: usize = 64 * 1024 * 1024;

#[allow(dead_code)]
#[derive(Clone, Copy)]
enum Wire<'a> {
    Varint(u64),
    Fixed64(&'a [u8]),
    Bytes(&'a [u8]),
    Fixed32(&'a [u8]),
}

#[derive(Clone, Default)]
struct ParagraphStyle {
    style: u32,
    indent: i32,
    done: bool,
    block_quote_level: u32,
}

#[derive(Clone, Default)]
struct AttributeRun {
    start: u32,
    end: u32,
    paragraph: ParagraphStyle,
    font_hints: u32,
    underline: bool,
    strikethrough: bool,
    link: String,
    attachment_id: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
enum ObjectValue<'a> {
    Ref(u64),
    Uint(u64),
    String(&'a [u8]),
}

pub fn render_body_html(encoded: &str, attachments: &[NoteAttachment]) -> AppResult<String> {
    let raw = decode_encoded(encoded)?;
    let document = versioned_data(&raw)?;
    let text = field_bytes(document, 2)
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .ok_or_else(|| AppError::other("Note body text is missing"))?
        .to_string();
    let runs = parse_attribute_runs(document, text.encode_utf16().count())?;
    render_paragraphs(&text, &runs, attachments)
}

pub fn parse_table(encoded: &str) -> AppResult<Vec<Vec<String>>> {
    let raw = decode_encoded(encoded)?;
    let document = versioned_data(&raw)?;
    table_grid(document)
}

fn decode_encoded(encoded: &str) -> AppResult<Vec<u8>> {
    let bytes =
        BASE64.decode(encoded).map_err(|error| AppError::other(format!("Could not decode note document: {error}")))?;
    let mut output = Vec::new();
    if bytes.starts_with(&[0x1f, 0x8b]) {
        GzDecoder::new(bytes.as_slice()).read_to_end(&mut output)?;
    } else if ZlibDecoder::new(bytes.as_slice()).read_to_end(&mut output).is_err() || output.is_empty() {
        output.clear();
        DeflateDecoder::new(bytes.as_slice()).read_to_end(&mut output)?;
    }
    if output.len() > MAX_DECODED_BYTES {
        return Err(AppError::other("Note document is too large"));
    }
    Ok(output)
}

fn versioned_data(raw: &[u8]) -> AppResult<&[u8]> {
    let version =
        field_bytes(raw, 2).ok_or_else(|| AppError::other("Versioned note document is missing version data"))?;
    field_bytes(version, 3).ok_or_else(|| AppError::other("Versioned note document is missing payload data"))
}

fn parse_attribute_runs(document: &[u8], text_length: usize) -> AppResult<Vec<AttributeRun>> {
    let mut runs = Vec::new();
    let mut offset = 0_u32;
    for raw_run in field_bytes_all(document, 5) {
        let length = field_varint(raw_run, 1).unwrap_or(0) as u32;
        let paragraph = field_bytes(raw_run, 2).map(parse_paragraph_style).unwrap_or_default();
        let attachment_id = field_bytes(raw_run, 12)
            .and_then(|info| field_bytes(info, 1))
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        runs.push(AttributeRun {
            start: offset,
            end: offset.saturating_add(length),
            paragraph,
            font_hints: field_varint(raw_run, 5).unwrap_or(0) as u32,
            underline: field_varint(raw_run, 6).unwrap_or(0) != 0,
            strikethrough: field_varint(raw_run, 7).unwrap_or(0) != 0,
            link: field_bytes(raw_run, 9)
                .and_then(|bytes| std::str::from_utf8(bytes).ok())
                .unwrap_or_default()
                .to_string(),
            attachment_id,
        });
        offset = offset.saturating_add(length);
    }
    if runs.is_empty() || offset as usize > text_length {
        return Ok(Vec::new());
    }
    Ok(runs)
}

fn parse_paragraph_style(data: &[u8]) -> ParagraphStyle {
    let todo = field_bytes(data, 5);
    ParagraphStyle {
        style: field_varint(data, 1).map_or(3, |value| value as u32),
        indent: field_varint(data, 4).map_or(0, |value| value as i32),
        done: todo.and_then(|value| field_varint(value, 2)).unwrap_or(0) != 0,
        block_quote_level: field_varint(data, 8).unwrap_or(0) as u32,
    }
}

fn render_paragraphs(text: &str, runs: &[AttributeRun], attachments: &[NoteAttachment]) -> AppResult<String> {
    let mut byte_to_utf16 = vec![0_usize; text.len() + 1];
    let mut utf16_offset = 0_usize;
    for (byte_offset, character) in text.char_indices() {
        byte_to_utf16[byte_offset] = utf16_offset;
        utf16_offset += character.len_utf16();
    }
    byte_to_utf16[text.len()] = utf16_offset;

    let mut html = String::from("<div class=\"note-content-inner\">");
    let mut line_start = 0_usize;
    let lines = text.split('\n').collect::<Vec<_>>();
    let line_count = lines.len();
    let mut numbered_counts: HashMap<i32, u32> = HashMap::new();
    for (line_index, line) in lines.into_iter().enumerate() {
        let line_end = line_start + line.len();
        let line_start_utf16 = byte_to_utf16[line_start] as u32;
        let line_end_utf16 = byte_to_utf16[line_end] as u32;
        let run = run_at(runs, line_start_utf16);
        let style = run.map(|candidate| candidate.paragraph.clone()).unwrap_or_default();

        if line.trim() == "\u{FFFC}" {
            if let Some(attachment_id) = run.and_then(|candidate| candidate.attachment_id.as_deref()) {
                if let Some(attachment) = attachments.iter().find(|candidate| candidate.id == attachment_id) {
                    if let Some(rendered) = render_attachment_html(attachment) {
                        html.push_str(&rendered);
                    }
                }
            }
            line_start = line_end + usize::from(line_index + 1 < line_count);
            continue;
        }

        let content = render_inline(text, line_start, line_end, line_start_utf16, line_end_utf16, runs, attachments);
        if line.is_empty() {
            html.push_str("<div class=\"note-empty\"></div>");
        } else {
            let wrapped = render_paragraph_content(&content, &style, &mut numbered_counts);
            if style.block_quote_level > 0 {
                write!(html, "<blockquote class=\"note-quote\">{wrapped}</blockquote>")
                    .expect("writing to string cannot fail");
            } else {
                html.push_str(&wrapped);
            }
        }
        line_start = line_end + usize::from(line_index + 1 < line_count);
    }
    html.push_str("</div>");
    Ok(html)
}

fn render_paragraph_content(content: &str, style: &ParagraphStyle, numbered_counts: &mut HashMap<i32, u32>) -> String {
    let indent = style.indent.clamp(0, 8);
    let indent_style = format!(" style=\"--note-indent:{indent}\"");
    match style.style {
        0 => format!("<h2 class=\"note-title\">{content}</h2>"),
        1 => format!("<h2 class=\"note-heading\">{content}</h2>"),
        2 => format!("<h3 class=\"note-subheading\">{content}</h3>"),
        4 => format!("<pre class=\"note-monospaced\">{content}</pre>"),
        100 | 101 => {
            let marker = if style.style == 101 { "–" } else { "•" };
            format!("<div class=\"note-list-item\"{indent_style}><span class=\"note-list-marker\">{marker}</span><span class=\"note-list-text\">{content}</span></div>")
        }
        102 => {
            let counter = numbered_counts.entry(indent).or_insert(1);
            let number = *counter;
            *counter += 1;
            format!("<div class=\"note-list-item\"{indent_style}><span class=\"note-list-marker note-list-number\">{number}.</span><span class=\"note-list-text\">{content}</span></div>")
        }
        103 => {
            let checked = if style.done { " checked" } else { "" };
            format!("<div class=\"note-list-item note-checklist\"{indent_style}><span class=\"note-checkbox{checked}\"></span><span class=\"note-list-text\">{content}</span></div>")
        }
        _ => format!("<p class=\"note-paragraph\">{content}</p>"),
    }
}

fn render_inline(
    text: &str,
    line_start: usize,
    line_end: usize,
    mut utf16: u32,
    line_end_utf16: u32,
    runs: &[AttributeRun],
    attachments: &[NoteAttachment],
) -> String {
    let mut html = String::new();
    for character in text[line_start..line_end].chars() {
        let units = character.len_utf16() as u32;
        if character == '\u{FFFC}' {
            if let Some(attachment_id) = run_at(runs, utf16).and_then(|run| run.attachment_id.as_deref()) {
                if let Some(attachment) = attachments.iter().find(|candidate| candidate.id == attachment_id) {
                    if let Some(rendered) = render_attachment_html(attachment) {
                        html.push_str(&rendered);
                    }
                }
            }
            utf16 += units;
            continue;
        }
        let run = run_at(runs, utf16);
        let mut value = escape_html(&character.to_string());
        if let Some(run) = run {
            if run.font_hints & 2 != 0 {
                value = format!("<em>{value}</em>");
            }
            if run.font_hints & 1 != 0 {
                value = format!("<strong>{value}</strong>");
            }
            if run.underline {
                value = format!("<u>{value}</u>");
            }
            if run.strikethrough {
                value = format!("<s>{value}</s>");
            }
            if let Some(href) = safe_link(&run.link) {
                value = format!("<a href=\"{href}\" target=\"_blank\" rel=\"noreferrer\">{value}</a>");
            }
        }
        html.push_str(&value);
        utf16 += units;
        if utf16 >= line_end_utf16 {
            break;
        }
    }
    html
}

fn render_attachment_html(attachment: &NoteAttachment) -> Option<String> {
    if let Some(rows) = attachment.table.as_ref() {
        return Some(render_table(rows));
    }

    let id = escape_html(&attachment.id);
    let uti = attachment.uti.as_deref().unwrap_or_default();
    let paper = uti == "com.apple.paper";
    let image = paper
        || uti.starts_with("public.image")
        || uti.ends_with(".png")
        || uti.ends_with(".jpeg")
        || uti.ends_with(".jpg")
        || uti.ends_with(".heic")
        || uti.ends_with(".gif")
        || uti.ends_with(".webp")
        || uti.ends_with(".tiff");
    if image {
        let url = if paper {
            attachment
                .render_url
                .as_deref()
                .or(attachment.fallback_url.as_deref())
                .or(attachment.preview_url.as_deref())
                .or(attachment.dark_preview_url.as_deref())
                .or(attachment.download_url.as_deref())
        } else {
            attachment
                .download_url
                .as_deref()
                .or(attachment.fallback_url.as_deref())
                .or(attachment.preview_url.as_deref())
                .or(attachment.dark_preview_url.as_deref())
        }?;
        let class = if paper { "note-embedded-image note-embedded-paper" } else { "note-embedded-image" };
        return Some(format!(
            "<figure class=\"{class}\" data-note-attachment-id=\"{id}\"><img src=\"{}\" alt=\"{}\" loading=\"lazy\" decoding=\"async\"></figure>",
            escape_html(url),
            escape_html(&attachment.name)
        ));
    }

    Some(format!(
        "<div class=\"note-file-embed\" data-note-attachment-id=\"{id}\"><span class=\"note-file-embed-name\">{}</span><span class=\"note-file-embed-type\">{}</span></div>",
        escape_html(&attachment.name),
        escape_html(uti)
    ))
}
fn render_table(rows: &[Vec<String>]) -> String {
    let mut html = String::from("<div class=\"note-table-wrap\"><table class=\"note-table\"><tbody>");
    for row in rows {
        html.push_str("<tr>");
        for cell in row {
            write!(html, "<td>{}</td>", escape_html(cell).replace('\n', "<br>"))
                .expect("writing to string cannot fail");
        }
        html.push_str("</tr>");
    }
    html.push_str("</tbody></table></div>");
    html
}

fn run_at(runs: &[AttributeRun], offset: u32) -> Option<&AttributeRun> {
    runs.iter().find(|run| offset >= run.start && offset < run.end)
}

fn safe_link(link: &str) -> Option<String> {
    let link = link.trim();
    if link.starts_with("https://") || link.starts_with("http://") || link.starts_with("mailto:") {
        Some(escape_html(link))
    } else {
        None
    }
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn table_grid(document: &[u8]) -> AppResult<Vec<Vec<String>>> {
    let objects = field_bytes_all(document, 3);
    let keys = field_bytes_all(document, 4)
        .into_iter()
        .filter_map(|bytes| std::str::from_utf8(bytes).ok().map(str::to_string))
        .collect::<Vec<_>>();
    let uuids = field_bytes_all(document, 6);
    if objects.is_empty() {
        return Err(AppError::other("Table object pool is empty"));
    }
    let table = custom_entries(objects[0], &keys).ok_or_else(|| AppError::other("Table root object is missing"))?;
    let cr_rows =
        table.get("crRows").and_then(resolve_ref).ok_or_else(|| AppError::other("Table is missing crRows"))?;
    let cr_columns =
        table.get("crColumns").and_then(resolve_ref).ok_or_else(|| AppError::other("Table is missing crColumns"))?;
    let cell_columns = table
        .get("cellColumns")
        .and_then(resolve_ref)
        .ok_or_else(|| AppError::other("Table is missing cellColumns"))?;
    let (row_ids, row_redirects) = ordered_set(&objects, &uuids, cr_rows)?;
    let (column_ids, column_redirects) = ordered_set(&objects, &uuids, cr_columns)?;
    let row_positions = positions(&objects, &keys, &row_ids, &row_redirects)?;
    let column_positions = positions(&objects, &keys, &column_ids, &column_redirects)?;
    if column_ids.is_empty() {
        return Err(AppError::other("Table has no columns"));
    }

    let mut grid = vec![vec![String::new(); column_ids.len()]; row_ids.len()];
    for (column_ref, row_map_ref) in dictionary_pairs(&objects, cell_columns)? {
        let Some(column_reference) = object_ref(&column_ref) else {
            continue;
        };
        let Ok(column_uuid) = uuid_index(&objects, &keys, column_reference) else {
            continue;
        };
        let column_position = column_positions.get(&column_uuid).copied();
        let Some(column_position) = column_position else {
            continue;
        };
        let Some(row_map_ref) = object_ref(&row_map_ref) else {
            continue;
        };
        for (row_ref, text_ref) in dictionary_pairs(&objects, row_map_ref)? {
            let Some(row_reference) = object_ref(&row_ref) else {
                continue;
            };
            let Ok(row_uuid) = uuid_index(&objects, &keys, row_reference) else {
                continue;
            };
            let row_position = row_positions.get(&row_uuid).copied();
            let Some(row_position) = row_position else {
                continue;
            };
            let Some(text_ref) = object_ref(&text_ref) else {
                continue;
            };
            if let Some(row) = grid.get_mut(row_position) {
                if let Some(cell) = row.get_mut(column_position) {
                    *cell = cell_text(&objects, text_ref);
                }
            }
        }
    }
    Ok(grid)
}

fn ordered_set(objects: &[&[u8]], uuids: &[&[u8]], reference: u64) -> AppResult<(Vec<u64>, Vec<(u64, u64)>)> {
    let object = pool_object(objects, reference)?;
    let ordered = field_bytes(object, 16).ok_or_else(|| AppError::other("Table ordered set is missing"))?;
    let array = field_bytes(ordered, 1).ok_or_else(|| AppError::other("Table ordered set array is missing"))?;
    let string_array = field_bytes(array, 1).ok_or_else(|| AppError::other("Table string array is missing"))?;
    let mut ids = Vec::new();
    for attachment in field_bytes_all(string_array, 2) {
        let uuid = field_bytes(attachment, 2).ok_or_else(|| AppError::other("Table array UUID is missing"))?;
        let index = uuids
            .iter()
            .position(|candidate| *candidate == uuid)
            .ok_or_else(|| AppError::other("Table array UUID is not in the UUID table"))?;
        ids.push(index as u64);
    }
    let mut redirects = Vec::new();
    if let Some(dictionary) = field_bytes(array, 2) {
        for element in field_bytes_all(dictionary, 1) {
            if let (Some(key), Some(value)) = (field_bytes(element, 1), field_bytes(element, 2)) {
                if let (Some(key), Some(value)) = (object_value(key), object_value(value)) {
                    if let (Some(key), Some(value)) = (object_ref(&key), object_ref(&value)) {
                        redirects.push((key, value));
                    }
                }
            }
        }
    }
    Ok((ids, redirects))
}

fn positions(
    objects: &[&[u8]],
    keys: &[String],
    ids: &[u64],
    redirects: &[(u64, u64)],
) -> AppResult<HashMap<u64, usize>> {
    let mut positions = ids.iter().enumerate().map(|(position, uuid)| (*uuid, position)).collect::<HashMap<_, _>>();
    for (key, value) in redirects {
        if let Some(position) = positions.get(&uuid_index(objects, keys, *key)?).copied() {
            positions.insert(uuid_index(objects, keys, *value)?, position);
        }
    }
    Ok(positions)
}

fn uuid_index(objects: &[&[u8]], keys: &[String], reference: u64) -> AppResult<u64> {
    let object = pool_object(objects, reference)?;
    let entries = custom_entries(object, keys).ok_or_else(|| AppError::other("Table identity object is missing"))?;
    match entries.get("UUIDIndex") {
        Some(ObjectValue::Uint(value)) => Ok(*value),
        _ => Err(AppError::other("Table identity object is missing UUIDIndex")),
    }
}

fn dictionary_pairs<'a>(objects: &'a [&'a [u8]], reference: u64) -> AppResult<Vec<(ObjectValue<'a>, ObjectValue<'a>)>> {
    let object = pool_object(objects, reference)?;
    let dictionary = field_bytes(object, 6).ok_or_else(|| AppError::other("Table dictionary is missing"))?;
    let mut pairs = Vec::new();
    for element in field_bytes_all(dictionary, 1) {
        let key = field_bytes(element, 1)
            .and_then(object_value)
            .ok_or_else(|| AppError::other("Table dictionary key is missing"))?;
        let value = field_bytes(element, 2)
            .and_then(object_value)
            .ok_or_else(|| AppError::other("Table dictionary value is missing"))?;
        pairs.push((key, value));
    }
    Ok(pairs)
}

fn cell_text(objects: &[&[u8]], reference: u64) -> String {
    pool_object(objects, reference)
        .ok()
        .and_then(|object| field_bytes(object, 10))
        .and_then(|text| field_bytes(text, 2))
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .unwrap_or_default()
        .to_string()
}

fn custom_entries<'a>(object: &'a [u8], keys: &[String]) -> Option<HashMap<String, ObjectValue<'a>>> {
    let custom = field_bytes(object, 13)?;
    let mut entries = HashMap::new();
    for entry in field_bytes_all(custom, 3) {
        let key_index = field_varint(entry, 1)? as usize;
        let value = field_bytes(entry, 2).and_then(object_value)?;
        if let Some(key) = keys.get(key_index) {
            entries.insert(key.clone(), value);
        }
    }
    Some(entries)
}

fn object_value(data: &[u8]) -> Option<ObjectValue<'_>> {
    if let Some(reference) = field_varint(data, 6) {
        Some(ObjectValue::Ref(reference))
    } else if let Some(number) = field_varint(data, 2) {
        Some(ObjectValue::Uint(number))
    } else {
        field_bytes(data, 4).map(ObjectValue::String)
    }
}

fn object_ref(value: &ObjectValue<'_>) -> Option<u64> {
    match value {
        ObjectValue::Ref(reference) => Some(*reference),
        _ => None,
    }
}

fn resolve_ref(value: &ObjectValue<'_>) -> Option<u64> {
    object_ref(value)
}

fn pool_object<'a>(objects: &'a [&'a [u8]], reference: u64) -> AppResult<&'a [u8]> {
    objects.get(reference as usize).copied().ok_or_else(|| AppError::other("Table object reference is out of range"))
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

fn field_bytes<'a>(data: &'a [u8], field: u64) -> Option<&'a [u8]> {
    parse_proto(data).ok()?.into_iter().find_map(|(candidate, _, value)| match value {
        Wire::Bytes(bytes) if candidate == field => Some(bytes),
        _ => None,
    })
}

fn field_bytes_all<'a>(data: &'a [u8], field: u64) -> Vec<&'a [u8]> {
    parse_proto(data)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(candidate, _, value)| match value {
            Wire::Bytes(bytes) if candidate == field => Some(bytes),
            _ => None,
        })
        .collect()
}

fn field_varint(data: &[u8], field: u64) -> Option<u64> {
    parse_proto(data).ok()?.into_iter().find_map(|(candidate, _, value)| match value {
        Wire::Varint(number) if candidate == field => Some(number),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_untrusted_text_and_links() {
        assert_eq!(escape_html("<script>"), "&lt;script&gt;");
        assert_eq!(safe_link("javascript:alert(1)"), None);
        assert_eq!(safe_link("https://example.com"), Some("https://example.com".to_string()));
    }
}
