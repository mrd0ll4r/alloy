use http::status::StatusCode;
use serde::{Deserialize, Serialize};
use crate::event::{EventFilterStrategy, EventFilterEntry};
use crate::config::VirtualDeviceConfig;

#[derive(Serialize, Deserialize, Debug)]
pub struct SubscriptionRequest {
    pub address: u16,
    pub strategy: EventFilterStrategy,
    pub filters: Vec<EventFilterEntry>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SetRequest {
    pub values: Vec<SetRequestInner>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SetRequestInner {
    Address { address: u16, value: u16 },
    Alias { alias: String, value: u16 },
    Group { group: String, value: u16 },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct APIResponse {
    pub ok: bool,
    pub status: u16,
    pub error: Option<String>,
    pub result: Option<APIResult>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum APIResult {
    Set,
    Get {
        value: u16,
    },
    Devices {
        virtual_devices: Vec<VirtualDeviceConfig>,
    },
}

impl APIResponse {
    pub fn from_result(result: APIResult) -> Self {
        APIResponse {
            ok: true,
            status: StatusCode::OK.as_u16(),
            error: None,
            result: Some(result),
        }
    }

    pub fn from_status(status: StatusCode) -> Self {
        APIResponse {
            ok: status.is_success(),
            status: status.as_u16(),
            error: if status.is_success() {
                None
            } else {
                Some(status.canonical_reason().unwrap().to_string())
            },
            result: None,
        }
    }

    pub fn from_error(e: failure::Error) -> Self {
        APIResponse {
            ok: false,
            status: StatusCode::BAD_REQUEST.as_u16(),
            error: Some(format!("{}", e)),
            result: None,
        }
    }
}

impl From<StatusCode> for APIResponse {
    fn from(status: StatusCode) -> Self {
        APIResponse::from_status(status)
    }
}

impl From<APIResult> for APIResponse {
    fn from(res: APIResult) -> Self {
        APIResponse::from_result(res)
    }
}
