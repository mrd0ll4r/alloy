use crate::config::UniverseConfig;
use crate::event::{AddressedEvent, EventFilterEntry, EventFilterStrategy};
use crate::{Address, OutputValue};
use serde::{Deserialize, Serialize};

// Protocol version used during handshake.
pub const PROTOCOL_VERSION: u16 = 4;

// All possible message types for TCP clients.
#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Version(u16),
    Config(UniverseConfig),
    Events(Vec<AddressedEvent>),
    Request {
        id: u16,
        inner: APIRequest,
    },
    Response {
        id: u16,
        inner: Result<APIResult, String>,
    },
}

// All possible request types for TCP clients.
#[derive(Serialize, Deserialize, Debug)]
pub enum APIRequest {
    Set(Vec<SetRequest>),
    Subscribe(SubscriptionRequest),
    Ping,
    SystemTime,
}

// The corresponding response types for TCP clients.
#[derive(Serialize, Deserialize, Debug)]
pub enum APIResult {
    Set,
    Subscribe,
    Ping,
    SystemTime(chrono::DateTime<chrono::Utc>),
}

// A subscription request for TCP clients.
#[derive(Serialize, Deserialize, Debug)]
pub struct SubscriptionRequest {
    pub address: Address,
    pub strategy: EventFilterStrategy,
    pub filters: Vec<EventFilterEntry>,
}

// A request to set a value for TCP clients.
#[derive(Serialize, Deserialize, Debug)]
pub struct SetRequest {
    pub value: OutputValue,
    pub address: Address,
}
