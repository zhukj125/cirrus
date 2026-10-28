pub mod auth {
    pub const BASE: &str = "https://idmsa.apple.com/appleauth/auth";
    pub const BASE_CN: &str = "https://idmsa.apple.com.cn/appleauth/auth";

    pub const BOOTSTRAP: &str = "/auth";
    pub const SIGNIN_INIT: &str = "/signin/init";
    pub const SIGNIN_COMPLETE: &str = "/signin/complete";

    pub const VERIFY_TRUSTED_DEVICE: &str = "/verify/trusteddevice/securitycode";

    pub const TRUST: &str = "/2sv/trust";

    pub const VERIFY_PHONE: &str = "/verify/phone";
    pub const VERIFY_PHONE_CODE: &str = "/verify/phone/securitycode";
    pub const PARAM_PHONE_NUMBER_ID: &str = "?phoneNumberId=";

    pub const BRIDGE_STEP_PREFIX: &str = "/bridge/step/";
    pub const BRIDGE_CODE_VALIDATE: &str = "/bridge/code/validate";
}

pub mod setup {
    pub const BASE: &str = "https://setup.icloud.com/setup/ws/1";
    pub const BASE_CN: &str = "https://setup.icloud.com.cn/setup/ws/1";
    pub const ACCOUNT_LOGIN: &str = "/accountLogin";
    pub const FMIP_WEB_AUTHENTICATE: &str = "/fmipWebAuthenticate";
}

pub mod contacts {
    pub const MECARD: &str = "/co/mecard/";
    pub const ADDRESSBOOK: &str = "/co/addressbook";
}

pub mod home {
    pub const URL: &str = "https://www.icloud.com";
    pub const URL_CN: &str = "https://www.icloud.com.cn";
}

pub mod find_my {
    pub const REFRESH_CLIENT: &str = "/fmipservice/client/web/refreshClient";
    pub const INIT_CLIENT: &str = "/fmipservice/client/web/initClient";
    pub const PLAY_SOUND: &str = "/fmipservice/client/web/playSound";

    pub const QUERY_CLIENT_BUILD_NUMBER: &str = "?clientBuildNumber=";
    pub const QUERY_CLIENT_MASTERING_NUMBER: &str = "&clientMasteringNumber=";
    pub const QUERY_CLIENT_ID: &str = "&clientId=";
    pub const QUERY_DSID: &str = "&dsid=";
    pub const SUBJECT_PLAY_SOUND: &str = "Find My iPhone Alert";

    pub const IMAGE_PATH: &str = "/fmipmobile/deviceImages-9.0";
    pub const IMAGE_CDN: &str = "https://statici.icloud.com/fmipmobile/deviceImages-9.0";
    pub const IMAGE_CDN_CN: &str = "https://statici.icloud.com.cn/fmipmobile/deviceImages-9.0";
    pub const IMAGE_VARIANT_ONLINE: &str = "online-sourcelist.png";
    pub const IMAGE_VARIANT_OFFLINE: &str = "offline-sourcelist.png";
}

pub mod json {
    pub const DS_INFO: &str = "dsInfo";
    pub const DSID: &str = "dsid";
    pub const FIRST_NAME: &str = "firstName";
    pub const LAST_NAME: &str = "lastName";
    pub const DOMAIN_TO_USE: &str = "domainToUse";
    pub const WEBSERVICES: &str = "webservices";
    pub const SERVICE_FINDME: &str = "findme";
    pub const SERVICE_PHOTOS: &str = "photos";
    pub const SERVICE_CONTACTS: &str = "contacts";
    pub const SERVICE_CKDATABASEWS: &str = "ckdatabasews";
    pub const URL: &str = "url";
    pub const CONTENT: &str = "content";
    pub const SERVER_CONTEXT: &str = "serverContext";
    pub const CLIENT_CONTEXT: &str = "clientContext";
    pub const FMLY: &str = "fmly";
    pub const SHOULD_LOCATE: &str = "shouldLocate";
    pub const SELECTED_DEVICE: &str = "selectedDevice";
    pub const SELECTED_DEVICE_ALL: &str = "all";
    pub const DEVICE_LIST_VERSION: &str = "deviceListVersion";
    pub const DEVICE: &str = "device";
    pub const SUBJECT: &str = "subject";
    pub const ALERT: &str = "alert";
    pub const IMAGE_BASE_URL: &str = "imageBaseUrl";
    pub const SUCCESS: &str = "success";
}

pub mod cloud_kit {
    pub const BASE: &str = "https://ckdatabasews.icloud.com";
    pub const BASE_CN: &str = "https://ckdatabasews.icloud.com.cn";
    pub const CONTAINER: &str = "com.apple.photos.cloud";

    pub const ZONE: &str = "PrimarySync";
    pub const ENVIRONMENT: &str = "production";
    pub const SCOPE_PRIVATE: &str = "private";

    pub const RECORDS_QUERY: &str = "/database/1/com.apple.photos.cloud/production/private/records/query";

    pub const RECORDS_LOOKUP: &str = "/database/1/com.apple.photos.cloud/production/private/records/lookup";

    pub const ZONES_LIST: &str = "/database/1/com.apple.photos.cloud/production/private/zones/list";

    pub const QUERY_PARAMS: &str = "?remapEnums=true&getCurrentSyncToken=true";
    pub const PARAM_DSID: &str = "&dsid=";
}
