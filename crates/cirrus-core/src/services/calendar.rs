use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use reqwest::{Method, Url};
use serde_json::{json, Map, Value};

use crate::apple::api;
use crate::apple::http::headers as hh;
use crate::error::{AppError, AppResult};
use crate::models::{
    CalendarCollection, CalendarEvent, CalendarEventDeleteResult, CalendarEventInput, CalendarEventMutationResult,
    CalendarLoadResult,
};
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

#[derive(Debug, Clone)]
struct LocalDateParts {
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
}

struct CalendarWriteContext {
    base: String,
    client_id: String,
    dsid: String,

    timezone_encoded: String,
    lang: String,
}

pub async fn save_event(
    manager: &SessionManager,
    session_id: &str,
    input: CalendarEventInput,
) -> CalendarEventMutationResult {
    if session_id.trim().is_empty() {
        return CalendarEventMutationResult::error("Missing session ID. Please sign in again.");
    }
    if input.calendar_id.trim().is_empty() {
        return CalendarEventMutationResult::error("Missing calendar ID");
    }
    if input.title.trim().is_empty() {
        return CalendarEventMutationResult::error("Event title is required");
    }
    let bounds = match event_query_bounds(&input.start, &input.end) {
        Ok(bounds) => bounds,
        Err(error) => return CalendarEventMutationResult::error(error.to_string()),
    };
    let handle = match manager.get(session_id).await {
        Ok(handle) => handle,
        Err(error) => return CalendarEventMutationResult::error(error.to_string()),
    };
    let mut session = handle.lock().await;
    if let Err(error) = devices::account_login(&mut session, false).await {
        return CalendarEventMutationResult::error(error.to_string());
    }
    let context = match calendar_write_context(&session, input.timezone.as_deref(), input.locale.as_deref()) {
        Ok(context) => context,
        Err(error) => return CalendarEventMutationResult::error(error.to_string()),
    };
    let event_id = input
        .event_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let existing = if input.event_id.as_deref().is_some_and(|value| !value.trim().is_empty()) {
        match fetch_calendar_event(&mut session, &context, &event_id, &input.calendar_id, &bounds).await {
            Ok(event) => {
                if event.get("recurrenceMaster").and_then(Value::as_bool) == Some(true) {
                    return CalendarEventMutationResult::error("Recurring events cannot be edited yet");
                }
                Some(event)
            }
            Err(error) => return CalendarEventMutationResult::error(error.to_string()),
        }
    } else {
        None
    };
    let ctag = match fetch_collection_ctag(&mut session, &context, &input.calendar_id, &bounds).await {
        Ok(ctag) => ctag,
        Err(error) => return CalendarEventMutationResult::error(error.to_string()),
    };
    let etag =
        existing.as_ref().and_then(|event| event.get("etag")).and_then(Value::as_str).unwrap_or_default().to_string();
    let body = match build_event_write_body(&input, &event_id, &etag, &ctag, existing.as_ref()) {
        Ok(body) => body,
        Err(error) => return CalendarEventMutationResult::error(error.to_string()),
    };
    let method_override = if existing.is_some() { Some("PUT") } else { None };
    let url = event_write_url(
        &context,
        &input.calendar_id,
        &event_id,
        &bounds,
        method_override,
        (!etag.is_empty()).then_some(etag.as_str()),
        1,
    );
    let response = match send_calendar_request(&mut session, &url, &body).await {
        Ok(response) => response,
        Err(error) => return CalendarEventMutationResult::error(error.to_string()),
    };
    if !response.is_success() {
        return CalendarEventMutationResult::error(format!(
            "Calendar save failed: HTTP {} {}",
            response.status,
            crate::services::auth::truncate(&response.body, 500)
        ));
    }
    let value: Value = match serde_json::from_str(&response.body) {
        Ok(value) => value,
        Err(error) => return CalendarEventMutationResult::error(format!("Calendar save response is invalid: {error}")),
    };
    if let Some(error) = calendar_response_error(&value) {
        return CalendarEventMutationResult::error(error);
    }
    let event = value
        .pointer("/ChangeSet/updates/Event")
        .and_then(Value::as_array)
        .and_then(|events| {
            events.iter().find(|event| event.get("guid").and_then(Value::as_str) == Some(event_id.as_str()))
        })
        .and_then(event_from_value);
    CalendarEventMutationResult::ok(event)
}

pub async fn delete_event(
    manager: &SessionManager,
    session_id: &str,
    event_id: &str,
    calendar_id: &str,
    start: &str,
    end: &str,
    timezone: Option<&str>,
    locale: Option<&str>,
) -> CalendarEventDeleteResult {
    if session_id.trim().is_empty() {
        return CalendarEventDeleteResult::error("Missing session ID. Please sign in again.");
    }
    if event_id.trim().is_empty() || calendar_id.trim().is_empty() {
        return CalendarEventDeleteResult::error("Missing event or calendar ID");
    }
    let bounds = match event_query_bounds(start, end) {
        Ok(bounds) => bounds,
        Err(error) => return CalendarEventDeleteResult::error(error.to_string()),
    };
    let handle = match manager.get(session_id).await {
        Ok(handle) => handle,
        Err(error) => return CalendarEventDeleteResult::error(error.to_string()),
    };
    let mut session = handle.lock().await;
    if let Err(error) = devices::account_login(&mut session, false).await {
        return CalendarEventDeleteResult::error(error.to_string());
    }
    let context = match calendar_write_context(&session, timezone, locale) {
        Ok(context) => context,
        Err(error) => return CalendarEventDeleteResult::error(error.to_string()),
    };
    let existing = match fetch_calendar_event(&mut session, &context, event_id, calendar_id, &bounds).await {
        Ok(event) => event,
        Err(error) => return CalendarEventDeleteResult::error(error.to_string()),
    };
    if existing.get("recurrenceMaster").and_then(Value::as_bool) == Some(true) {
        return CalendarEventDeleteResult::error("Recurring events cannot be deleted yet");
    }
    let etag = existing.get("etag").and_then(Value::as_str).unwrap_or_default();
    if etag.is_empty() {
        return CalendarEventDeleteResult::error("Calendar event is missing etag");
    }
    let ctag = match fetch_collection_ctag(&mut session, &context, calendar_id, &bounds).await {
        Ok(ctag) => ctag,
        Err(error) => return CalendarEventDeleteResult::error(error.to_string()),
    };
    let body = json!({
        "Event": {},
        "ClientState": { "Collection": [{ "ctag": ctag, "guid": calendar_id }] }
    });
    let url = event_write_url(&context, calendar_id, event_id, &bounds, Some("DELETE"), Some(etag), 1);
    let response = match send_calendar_request(&mut session, &url, &body).await {
        Ok(response) => response,
        Err(error) => return CalendarEventDeleteResult::error(error.to_string()),
    };
    if !response.is_success() {
        return CalendarEventDeleteResult::error(format!(
            "Calendar delete failed: HTTP {} {}",
            response.status,
            crate::services::auth::truncate(&response.body, 500)
        ));
    }
    if let Ok(value) = serde_json::from_str::<Value>(&response.body) {
        if let Some(error) = calendar_response_error(&value) {
            return CalendarEventDeleteResult::error(error);
        }
    }
    CalendarEventDeleteResult::ok(event_id.to_string())
}

fn calendar_write_context(
    session: &crate::session::SrpSession,
    timezone: Option<&str>,
    locale: Option<&str>,
) -> AppResult<CalendarWriteContext> {
    let base = calendar_base(session)?;
    let timezone = timezone.map(str::trim).filter(|value| !value.is_empty()).unwrap_or("Asia/Shanghai").to_string();
    let locale = locale.map(str::trim).filter(|value| !value.is_empty()).unwrap_or("en-US");
    let lang = if locale.to_ascii_lowercase().starts_with("zh") { "zh-CN" } else { "en-US" };
    Ok(CalendarWriteContext {
        base,
        client_id: session.client_id().to_string(),
        dsid: session.dsid().unwrap_or("null").to_string(),
        timezone_encoded: utf8_percent_encode(&timezone, NON_ALPHANUMERIC).to_string(),

        lang: lang.to_string(),
    })
}

fn event_write_url(
    context: &CalendarWriteContext,
    calendar_id: &str,
    event_id: &str,
    bounds: &(String, String),
    method_override: Option<&str>,
    if_match: Option<&str>,
    request_id: i32,
) -> String {
    let override_part = method_override.map(|value| format!("&methodOverride={value}")).unwrap_or_default();
    let match_part = if_match.map(|value| format!("&ifMatch={value}")).unwrap_or_default();
    format!(
        "{base}{events}/{calendar_id}/{event_id}?lang={lang}&usertz={timezone}&requestID={request_id}{override_part}{match_part}&startDate={start}&endDate={end}&clientBuildNumber={build}&clientMasteringNumber={build}&clientId={client_id}&dsid={dsid}",
        base = context.base,
        events = api::calendar::EVENTS,
        lang = context.lang,
        timezone = context.timezone_encoded,
        start = bounds.0,
        end = bounds.1,
        build = CALENDAR_BUILD_NUMBER,
        client_id = context.client_id,
        dsid = context.dsid,
    )
}

async fn send_calendar_request(
    session: &mut crate::session::SrpSession,
    url: &str,
    body: &Value,
) -> AppResult<crate::session::HttpResponse> {
    let headers = [(hh::ACCEPT, "*/*"), (hh::CONTENT_TYPE, "text/plain")];
    session.request(Method::POST, url, Some(body), &headers).await
}

async fn fetch_calendar_event(
    session: &mut crate::session::SrpSession,
    context: &CalendarWriteContext,
    event_id: &str,
    calendar_id: &str,
    bounds: &(String, String),
) -> AppResult<Value> {
    let query = format!(
        "startDate={start}&endDate={end}&usertz={timezone}&lang={lang}&requestID=2&clientVersion={CALENDAR_CLIENT_VERSION}&clientBuildNumber={build}&clientMasteringNumber={build}&clientId={client_id}&dsid={dsid}",
        start = bounds.0,
        end = bounds.1,
        timezone = context.timezone_encoded,
        lang = context.lang,
        build = CALENDAR_BUILD_NUMBER,
        client_id = context.client_id,
        dsid = context.dsid,
    );
    let url = format!("{}{}?{}", context.base, api::calendar::EVENTS, query);
    let response =
        session.request(Method::GET, &url, None, &[(hh::ACCEPT, "*/*"), (hh::CONTENT_TYPE, "text/plain")]).await?;
    if !response.is_success() {
        return Err(AppError::other(format!(
            "Calendar event lookup failed: HTTP {} {}",
            response.status,
            crate::services::auth::truncate(&response.body, 400)
        )));
    }
    let value: Value = serde_json::from_str(&response.body)?;
    value
        .get("Event")
        .and_then(Value::as_array)
        .and_then(|events| {
            events.iter().find(|event| {
                event.get("guid").and_then(Value::as_str) == Some(event_id)
                    && event.get("pGuid").and_then(Value::as_str) == Some(calendar_id)
            })
        })
        .cloned()
        .ok_or_else(|| AppError::other("Calendar event was not found"))
}

async fn fetch_collection_ctag(
    session: &mut crate::session::SrpSession,
    context: &CalendarWriteContext,
    calendar_id: &str,
    bounds: &(String, String),
) -> AppResult<String> {
    let query = format!(
        "startDate={start}&endDate={end}&usertz={timezone}&lang={lang}&requestID=1&clientVersion={CALENDAR_CLIENT_VERSION}&clientBuildNumber={build}&clientMasteringNumber={build}&clientId={client_id}&dsid={dsid}",
        start = bounds.0,
        end = bounds.1,
        timezone = context.timezone_encoded,
        lang = context.lang,
        build = CALENDAR_BUILD_NUMBER,
        client_id = context.client_id,
        dsid = context.dsid,
    );
    let url = format!("{}{}?{}", context.base, api::calendar::ALL_COLLECTIONS, query);
    let response =
        session.request(Method::GET, &url, None, &[(hh::ACCEPT, "*/*"), (hh::CONTENT_TYPE, "text/plain")]).await?;
    if !response.is_success() {
        return Err(AppError::other(format!("Calendar collections request failed: HTTP {}", response.status)));
    }
    let value: Value = serde_json::from_str(&response.body)?;
    value
        .get("Collection")
        .and_then(Value::as_array)
        .and_then(|collections| {
            collections.iter().find(|collection| collection.get("guid").and_then(Value::as_str) == Some(calendar_id))
        })
        .and_then(|collection| collection.get("ctag"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| AppError::other("Calendar collection is missing ctag"))
}

fn build_event_write_body(
    input: &CalendarEventInput,
    event_id: &str,
    etag: &str,
    ctag: &str,
    existing: Option<&Value>,
) -> AppResult<Value> {
    let start = parse_local_datetime(&input.start)?;
    let end = parse_local_datetime(&input.end)?;
    let start_array = date_array(&start);
    let end_array = date_array(&end);
    let duration = event_duration(&start, &end, input.all_day).max(1);
    let timezone = input.timezone.as_deref().unwrap_or("Asia/Shanghai");
    let offset = input.timezone_offset.as_deref().unwrap_or("+08:00");
    let mut event = existing.and_then(Value::as_object).cloned().unwrap_or_default();
    if existing.is_none() {
        event.insert("createdDate".to_string(), start_array.clone());
        event.insert("hasAttachments".to_string(), json!(false));
        event.insert("alarms".to_string(), json!([]));
        event.insert("attachments".to_string(), json!([]));
        event.insert("invitees".to_string(), json!([]));
        event.insert("birthdayIsYearlessBday".to_string(), json!(false));
        event.insert("birthdayShowAsCompany".to_string(), json!(false));
        event.insert("shouldShowJunkUIWhenAppropriate".to_string(), json!(false));
    }
    event.insert("title".to_string(), json!(input.title.trim()));
    event.insert("tz".to_string(), json!(timezone));
    event.insert("tzname".to_string(), json!(timezone_name(timezone)));
    event.insert("icon".to_string(), json!(0));
    event.insert("recurrenceException".to_string(), json!(false));
    event.insert("duration".to_string(), json!(duration));
    event.insert("allDay".to_string(), json!(input.all_day));
    event.insert("startDateTZOffset".to_string(), json!(offset));
    event.insert("pGuid".to_string(), json!(input.calendar_id));
    event.insert("readOnly".to_string(), json!(false));
    event.insert("lastModifiedDate".to_string(), start_array.clone());
    event.insert("localStartDate".to_string(), start_array.clone());
    event.insert("localEndDate".to_string(), end_array.clone());
    event.insert("startDate".to_string(), start_array.clone());
    event.insert("endDate".to_string(), end_array.clone());
    event.insert("masterStartDate".to_string(), start_array);
    event.insert("masterEndDate".to_string(), end_array);
    event.insert("guid".to_string(), json!(event_id));
    event.insert("etag".to_string(), json!(etag));
    event.insert("recurrenceMaster".to_string(), json!(false));
    event.insert("transparent".to_string(), json!(false));
    event.insert("extendedDetailsAreIncluded".to_string(), json!(true));
    event.insert("description".to_string(), json!(input.notes));
    event.insert("url".to_string(), json!(input.url.trim()));
    event.insert("location".to_string(), json!(input.location.trim()));
    Ok(json!({
        "Event": Value::Object(event),
        "Invitee": [],
        "Alarm": [],
        "ClientState": { "Collection": [{ "ctag": ctag, "guid": input.calendar_id }] }
    }))
}

fn parse_local_datetime(value: &str) -> AppResult<LocalDateParts> {
    let value = value.trim();
    let (date, time) = value.split_once('T').unwrap_or((value, "00:00"));
    let mut date_parts = date.split('-');
    let year = date_parts
        .next()
        .and_then(|part| part.parse::<i32>().ok())
        .ok_or_else(|| AppError::other("Invalid event start/end date"))?;
    let month = date_parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .ok_or_else(|| AppError::other("Invalid event start/end date"))?;
    let day = date_parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .ok_or_else(|| AppError::other("Invalid event start/end date"))?;
    let time = time.split(['+', 'Z']).next().unwrap_or(time);
    let mut time_parts = time.split(':');
    let hour = time_parts.next().and_then(|part| part.parse::<u32>().ok()).unwrap_or(0);
    let minute = time_parts.next().and_then(|part| part.parse::<u32>().ok()).unwrap_or(0);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return Err(AppError::other("Invalid event start/end date"));
    }
    Ok(LocalDateParts { year, month, day, hour, minute })
}

fn date_array(parts: &LocalDateParts) -> Value {
    let date_key = (parts.year as i64) * 10_000 + (parts.month as i64) * 100 + parts.day as i64;
    json!([date_key, parts.year, parts.month, parts.day, parts.hour, parts.minute, parts.hour * 60 + parts.minute])
}

fn event_duration(start: &LocalDateParts, end: &LocalDateParts, all_day: bool) -> i64 {
    let start_minutes =
        days_from_civil(start.year, start.month, start.day) * 1_440 + start.hour as i64 * 60 + start.minute as i64;
    let end_minutes = days_from_civil(end.year, end.month, end.day) * 1_440 + end.hour as i64 * 60 + end.minute as i64;
    if all_day {
        ((end_minutes - start_minutes) / 1_440).max(1) * 1_440
    } else {
        end_minutes - start_minutes
    }
}

fn event_query_bounds(start: &str, end: &str) -> AppResult<(String, String)> {
    let start = parse_local_datetime(start)?;
    let end = parse_local_datetime(end)?;
    let start_date = format!("{:04}-{:02}-{:02}", start.year, start.month, start.day);
    let end_days = days_from_civil(end.year, end.month, end.day) + 1;
    let (year, month, day) = civil_from_days(end_days);
    Ok((start_date, format!("{year:04}-{month:02}-{day:02}")))
}

fn timezone_name(timezone: &str) -> &str {
    if timezone == "Asia/Shanghai" {
        "CST"
    } else if timezone.eq_ignore_ascii_case("UTC") {
        "UTC"
    } else {
        timezone.rsplit('/').next().unwrap_or("UTC")
    }
}

fn calendar_response_error(value: &Value) -> Option<String> {
    for key in ["Error", "error", "errorCode", "message"] {
        if let Some(error) = value.get(key).filter(|value| !value.is_null()) {
            let message = error.as_str().map(str::to_string).unwrap_or_else(|| error.to_string());
            if !message.trim().is_empty() {
                return Some(message);
            }
        }
    }
    None
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - i32::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = month as i32;
    let day = day as i32;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era as i64 * 146_097 + day_of_era as i64 - 719_468
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era as i32 + era as i32 * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i32::from(month <= 2);
    (year, month as u32, day as u32)
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
    #[test]
    fn builds_create_event_payload_with_event_dates() {
        let input = CalendarEventInput {
            event_id: None,
            calendar_id: "cal-1".to_string(),
            title: "新建日程".to_string(),
            start: "2026-11-04T03:30".to_string(),
            end: "2026-11-04T04:30".to_string(),
            all_day: false,
            location: "会议室".to_string(),
            notes: "备注".to_string(),
            url: "https://example.com".to_string(),
            timezone: Some("Asia/Shanghai".to_string()),
            timezone_offset: Some("+08:00".to_string()),
            locale: Some("zh-CN".to_string()),
        };
        let body = build_event_write_body(&input, "event-1", "", "ctag-1", None).expect("event body");
        assert_eq!(body["Event"]["title"], "新建日程");
        assert_eq!(body["Event"]["duration"], 60);
        assert_eq!(body["Event"]["pGuid"], "cal-1");
        assert_eq!(body["Event"]["startDate"][0], 20261104);
        assert_eq!(body["Event"]["startDate"][6], 210);
        assert_eq!(body["ClientState"]["Collection"][0]["ctag"], "ctag-1");
    }

    #[test]
    fn event_query_range_includes_the_end_day() {
        let bounds = event_query_bounds("2026-11-04T03:30", "2026-11-04T04:30").expect("bounds");
        assert_eq!(bounds.0, "2026-11-04");
        assert_eq!(bounds.1, "2026-11-05");
    }
}
