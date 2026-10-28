use reqwest::Method;
use serde_json::{Map, Value};

use crate::apple::api;
use crate::apple::http::headers as hh;
use crate::error::{AppError, AppResult};
use crate::models::{ContactAddress, ContactEntry, ContactField, ContactsListResult};
use crate::services::devices;
use crate::SessionManager;

const CONTACT_BUILD_NUMBER: &str = "2640Build24";
const CONTACT_CLIENT_VERSION: &str = "2.1";

pub async fn list(manager: &SessionManager, session_id: &str, locale: &str) -> ContactsListResult {
    if session_id.trim().is_empty() {
        return ContactsListResult::error("Missing session ID. Please sign in again.");
    }
    let handle = match manager.get(session_id).await {
        Ok(handle) => handle,
        Err(error) => return ContactsListResult::error(error.to_string()),
    };
    let mut session = handle.lock().await;
    if let Err(error) = devices::account_login(&mut session, false).await {
        return ContactsListResult::error(error.to_string());
    }
    let Some(base) = session.contacts_url().map(str::to_string) else {
        return ContactsListResult::error("Contacts is not available for this account");
    };
    let client_id = session.client_id().to_string();
    let dsid = session.dsid().unwrap_or("null").to_string();
    let locale = if locale.to_ascii_lowercase().starts_with("zh") { "zh_CN" } else { "en_US" };
    let url = format!(
        "{base}{}?locale={locale}&order=first%2Clast&includePhoneNumbers=true&includePhotos=true&clientBuildNumber={CONTACT_BUILD_NUMBER}&clientMasteringNumber={CONTACT_BUILD_NUMBER}&clientVersion={CONTACT_CLIENT_VERSION}&clientId={client_id}&dsid={dsid}",
        api::contacts::ADDRESSBOOK
    );
    let headers = [(hh::ACCEPT, "*/*"), (hh::CONTENT_TYPE, "text/plain")];
    let response = match session.request(Method::GET, &url, None, &headers).await {
        Ok(response) => response,
        Err(error) => return ContactsListResult::error(error.to_string()),
    };
    if !(200..300).contains(&response.status) {
        return ContactsListResult::error(format!(
            "Contacts request failed: HTTP {} {}",
            response.status,
            crate::services::auth::truncate(&response.body, 400)
        ));
    }
    match parse_addressbook(&response.body) {
        Ok(contacts) => ContactsListResult::ok(contacts),
        Err(error) => ContactsListResult::error(error.to_string()),
    }
}

fn parse_addressbook(body: &str) -> AppResult<Vec<ContactEntry>> {
    let response: Value = serde_json::from_str(body)?;
    let contacts = response
        .get("contacts")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::other("Contacts response is missing the contacts list"))?;
    Ok(contacts.iter().filter_map(contact_from_value).collect())
}

fn contact_from_value(value: &Value) -> Option<ContactEntry> {
    let object = value.as_object()?;
    let first_name = string_value(object, &["firstName"]);
    let last_name = string_value(object, &["lastName"]);
    let middle_name = string_value(object, &["middleName"]);
    let nickname = string_value(object, &["nickname"]);
    let company_name = string_value(object, &["companyName", "company"]);
    let department = string_value(object, &["department"]);
    let job_title = string_value(object, &["jobTitle", "title"]);
    let is_company = object.get("isCompany").and_then(Value::as_bool).unwrap_or(false);
    let display_name = if is_company && !company_name.is_empty() {
        company_name.clone()
    } else {
        display_name(&first_name, &middle_name, &last_name, &nickname, &company_name)
    };
    if display_name.is_empty() && string_value(object, &["normalized"]).is_empty() && !has_contact_fields(object) {
        return None;
    }
    let id = string_value(object, &["contactId"]);
    let id = if id.is_empty() {
        let etag = string_value(object, &["etag"]);
        format!("{display_name}:{etag}")
    } else {
        id
    };
    let photo_url = object
        .get("photo")
        .and_then(Value::as_object)
        .and_then(|photo| photo.get("url"))
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
        .map(str::to_string);
    Some(ContactEntry {
        id,
        display_name: if display_name.is_empty() { "Unnamed contact".to_string() } else { display_name },
        first_name,
        last_name,
        middle_name,
        nickname,
        company_name,
        department,
        job_title,
        phones: contact_fields(object, "phones"),
        emails: contact_fields(object, "emailAddresses"),
        addresses: contact_addresses(object),
        urls: contact_fields(object, "urls"),
        related_names: contact_fields(object, "relatedNames"),
        birthday: object.get("birthday").and_then(display_value),
        note: object.get("notes").or_else(|| object.get("note")).and_then(display_value),
        photo_url,
        is_company,
    })
}

fn display_name(first: &str, middle: &str, last: &str, nickname: &str, company: &str) -> String {
    if !first.is_empty() && !last.is_empty() && is_cjk(first) && is_cjk(last) {
        return format!("{last}{first}");
    }
    let name = [first, middle, last].into_iter().filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" ");
    if !name.is_empty() {
        return name;
    }
    if !nickname.is_empty() {
        return nickname.to_string();
    }
    company.to_string()
}

fn is_cjk(value: &str) -> bool {
    value.chars().next().is_some_and(|character| matches!(character as u32, 0x3400..=0x9fff | 0xf900..=0xfaff))
}

fn has_contact_fields(object: &Map<String, Value>) -> bool {
    ["phones", "emailAddresses", "urls", "streetAddresses", "relatedNames"]
        .into_iter()
        .any(|key| object.get(key).and_then(Value::as_array).is_some_and(|values| !values.is_empty()))
}

fn string_value(object: &Map<String, Value>, keys: &[&str]) -> String {
    keys.iter().find_map(|key| object.get(*key).and_then(display_value)).unwrap_or_default()
}

fn display_value(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.trim().to_string()).filter(|value| !value.is_empty()),
        Value::Number(value) => Some(value.to_string()),
        Value::Object(object) => {
            ["date", "value", "field", "name"].into_iter().find_map(|key| object.get(key).and_then(display_value))
        }
        _ => None,
    }
}

fn contact_fields(object: &Map<String, Value>, key: &str) -> Vec<ContactField> {
    object
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter_map(|entry| {
            let value = string_value(entry, &["field", "value", "address", "number"]);
            if value.is_empty() {
                return None;
            }
            let label = string_value(entry, &["label", "type"]);
            Some(ContactField { label: if label.is_empty() { "other".to_string() } else { label }, value })
        })
        .collect()
}

fn contact_addresses(object: &Map<String, Value>) -> Vec<ContactAddress> {
    object
        .get("streetAddresses")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter_map(|address| {
            let label = string_value(address, &["label", "type"]);
            let street = string_value(address, &["street", "streetAddress"]);
            let city = string_value(address, &["city"]);
            let region = string_value(address, &["state", "stateProvince", "region"]);
            let postal_code = string_value(address, &["postalCode", "zip", "postal"]);
            let country = string_value(address, &["country", "countryName"]);
            if street.is_empty() && city.is_empty() && region.is_empty() && postal_code.is_empty() && country.is_empty()
            {
                return None;
            }
            Some(ContactAddress {
                label: if label.is_empty() { "other".to_string() } else { label },
                street,
                city,
                region,
                postal_code,
                country,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_contact_fields_and_addresses() {
        let body = r#"{"contacts":[{"contactId":"1","firstName":"三","lastName":"张","phones":[{"field":"13800000000","label":"MOBILE"}],"emailAddresses":[{"field":"a@example.com","label":"HOME"}],"streetAddresses":[{"label":"HOME","street":"中山路 1 号","city":"杭州","state":"浙江","postalCode":"310000","country":"中国"}],"photo":{"url":"https://example.com/a.jpg"},"isCompany":false}]}"#;
        let contacts = parse_addressbook(body).expect("contacts");
        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts[0].display_name, "张三");
        assert_eq!(contacts[0].phones[0].value, "13800000000");
        assert_eq!(contacts[0].addresses[0].city, "杭州");
        assert_eq!(contacts[0].photo_url.as_deref(), Some("https://example.com/a.jpg"));
    }

    #[test]
    fn keeps_company_contacts_and_deduplicates_empty_rows() {
        let body = r#"{"contacts":[{"contactId":"1","companyName":"Acme","isCompany":true,"phones":[{"field":"10086"}]},{"contactId":"2","isCompany":false}]}"#;
        let contacts = parse_addressbook(body).expect("contacts");
        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts[0].display_name, "Acme");
    }
}
