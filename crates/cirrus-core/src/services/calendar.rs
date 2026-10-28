use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use reqwest::{Method, Url};
use serde_json::{Map, Value};

use crate::apple::api;
use crate::apple::http::headers as hh;
use crate::error::{AppError, AppResult};
use crate::models::{CalendarCollection, CalendarEvent, CalendarLoadResult};
use crate::services::devices;
use crate::SessionManager;

const CALENDAR_BUILD_NUMBER: &str = "2640Build24";
const CALENDAR_CLIENT_VERSION: &str = "6.0";

pub async fn load(
    manager: &SessionManager,
    session_id: &str,
    start_date: &str,
    end_date: &str,
    timezone: &str,
    locale: &str,
) -> CalendarLoadResult {
    if session_id.trim().is_empty() {
        return CalendarLoadResult::error("Missing session ID. Please sign in again.");
    }
    let handle = match manager.get(session_id).await {
        Ok(handle) => handle,
        Err(error) => return CalendarLoadResult::error(error.to_string()),
    };
    let mut session = handle.lock().await;
    if let Err(error) = devices::account_login(&mut session, false).await {
        return CalendarLoadResult::error(error.to_string());
    }
    let base = match calendar_base(&session) {
        Ok(base) => base,
        Err(error) => return CalendarLoadResult::error(error.to_string()),
    };
    let client_id = session.client_id().to_string();
    let dsid = session.dsid().unwrap_or("null").to_string();
    let timezone = utf8_percent_encode(timezone, NON_ALPHANUMERIC).to_string();
    let lang = if locale.to_ascii_lowercase().starts_with("zh") { "zh-CN" } else { "en-US" };
    let query = format!(
        "startDate={start_date}&endDate={end_date}&usertz={timezone}&lang={lang}&clientVersion={CALENDAR_CLIENT_VERSION}&clientBuildNumber={CALENDAR_BUILD_NUMBER}&clientMasteringNumber={CALENDAR_BUILD_NUMBER}&clientId={client_id}&dsid={dsid}"
    );
    let headers = [(hh::ACCEPT, "*/*"), (hh::CONTENT_TYPE, "text/plain")];

    let collections_url = format!("{base}{}?requestID=1&{query}", api::calendar::ALL_COLLECTIONS);
    let collections = match session.request(Method::GET, &collections_url, None, &headers).await {
        Ok(response) if response.is_success() => parse_collections(&response.body),
        Ok(response) => Err(AppError::other(format!(
            "Calendar collections request failed: HTTP {} {}",
            response.status,
            crate::services::auth::truncate(&response.body, 400)
        ))),
        Err(error) => Err(error),
    };
    let collections = match collections {
        Ok(calendars) => calendars,
        Err(error) => return CalendarLoadResult::error(error.to_string()),
    };

    let events_url = format!("{base}{}?requestID=2&{query}", api::calendar::EVENTS);
    let events = match session.request(Method::GET, &events_url, None, &headers).await {
        Ok(response) if response.is_success() => parse_events(&response.body),
        Ok(response) => Err(AppError::other(format!(
            "Calendar events request failed: HTTP {} {}",
            response.status,
            crate::services::auth::truncate(&response.body, 400)
        ))),
        Err(error) => Err(error),
    };
    match events {
        Ok(events) => CalendarLoadResult::ok(collections, events),
        Err(error) => CalendarLoadResult::error(error.to_string()),
    }
}

fn calendar_base(session: &crate::session::SrpSession) -> AppResult<String> {
    let contacts =
        session.contacts_url().ok_or_else(|| AppError::other("accountLogin did not return a contacts service URL"))?;
    let mut url =
        Url::parse(contacts).map_err(|error| AppError::other(format!("Invalid contacts service URL: {error}")))?;
    let host = url
        .host_str()
        .ok_or_else(|| AppError::other("Contacts service URL has no host"))?
        .replace("contactsws", "calendarws");
    if host == url.host_str().unwrap_or_default() {
        return Err(AppError::other("Could not derive the Calendar service URL"));
    }
    url.set_host(Some(&host)).map_err(|error| AppError::other(format!("Invalid calendar service host: {error}")))?;
    url.set_path("");
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string().trim_end_matches('/').to_string())
}

fn parse_collections(body: &str) -> AppResult<Vec<CalendarCollection>> {
    let response: Value = serde_json::from_str(body)?;
    let values = response
        .get("Collection")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::other("Calendar response is missing collections"))?;
    let mut calendars = values.iter().filter_map(collection_from_value).collect::<Vec<_>>();
    calendars.sort_by_key(|calendar| calendar.order);
    Ok(calendars)
}

fn collection_from_value(value: &Value) -> Option<CalendarCollection> {
    let object = value.as_object()?;
    let id = string_field(object, &["guid", "id"])?;
    let title = string_field(object, &["title", "shareTitle"]).unwrap_or_else(|| "Calendar".to_string());
    Some(CalendarCollection {
        id,
        title,
        color: string_field(object, &["color"]).unwrap_or_else(|| "#1a73e8".to_string()),
        enabled: bool_field(object, "enabled").unwrap_or(true),
        read_only: bool_field(object, "readOnly").unwrap_or(false),
        is_default: bool_field(object, "isDefault").unwrap_or(false),
        order: integer_field(object, "order").unwrap_or(0),
    })
}

fn parse_events(body: &str) -> AppResult<Vec<CalendarEvent>> {
    let response: Value = serde_json::from_str(body)?;
    let values = response
        .get("Event")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::other("Calendar response is missing events"))?;
    let mut events = values.iter().filter_map(event_from_value).collect::<Vec<_>>();
    events.sort_by(|left, right| left.start.cmp(&right.start).then_with(|| left.title.cmp(&right.title)));
    Ok(events)
}

fn event_from_value(value: &Value) -> Option<CalendarEvent> {
    let object = value.as_object()?;
    let id = string_field(object, &["guid", "id"])?;
    let start = date_field(object, "startDate")?;
    let end = date_field(object, "endDate").unwrap_or_else(|| start.clone());
    Some(CalendarEvent {
        id,
        title: string_field(object, &["title"]).unwrap_or_else(|| "Untitled event".to_string()),
        calendar_id: string_field(object, &["pGuid", "calendarGuid"]).unwrap_or_default(),
        start,
        end,
        all_day: bool_field(object, "allDay").unwrap_or(false),
        recurring: bool_field(object, "recurrenceMaster").unwrap_or(false),
        read_only: bool_field(object, "readOnly").unwrap_or(false),
        location: string_field(object, &["location", "locationTitle"]).unwrap_or_default(),
        notes: string_field(object, &["notes", "description", "comment"]).unwrap_or_default(),
        url: string_field(object, &["url"]).unwrap_or_default(),
    })
}

fn date_field(object: &Map<String, Value>, key: &str) -> Option<String> {
    let value = object.get(key)?;
    if let Some(text) = value.as_str().filter(|text| !text.is_empty()) {
        return Some(text.to_string());
    }
    let values = value.as_array()?;
    let year = values.get(1)?.as_i64()?;
    let month = values.get(2)?.as_i64()?;
    let day = values.get(3)?.as_i64()?;
    let hour = values.get(4).and_then(Value::as_i64).unwrap_or(0);
    let minute = values.get(5).and_then(Value::as_i64).unwrap_or(0);
    Some(format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:00"))
}

fn string_field(object: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        object.get(*key).and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty()).map(str::to_string)
    })
}

fn bool_field(object: &Map<String, Value>, key: &str) -> Option<bool> {
    object.get(key).and_then(|value| value.as_bool().or_else(|| value.as_i64().map(|number| number != 0)))
}

fn integer_field(object: &Map<String, Value>, key: &str) -> Option<i64> {
    object.get(key).and_then(|value| value.as_i64().or_else(|| value.as_f64().map(|number| number as i64)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_calendars_and_event_dates() {
        let collections = parse_collections(
            r##"{"Collection":[{"guid":"cal-1","title":"日历","color":"#cc73e1","enabled":true,"order":0}]}"##,
        )
        .expect("collections");
        assert_eq!(collections[0].title, "日历");
        let events = parse_events(
            r#"{"Event":[{"guid":"evt-1","title":"新建日程","pGuid":"cal-1","startDate":[20261028,2026,10,28,9,30,570],"endDate":[20261028,2026,10,28,10,30,630],"allDay":false}]}"#,
        )
        .expect("events");
        assert_eq!(events[0].start, "2026-10-28T09:30:00");
        assert_eq!(events[0].calendar_id, "cal-1");
    }
}
