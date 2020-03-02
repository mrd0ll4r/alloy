use crate::config::VirtualDeviceConfig;
use crate::event::{AddressedEvent, EventFilterEntry, EventFilterStrategy};
use crate::{Address, Value};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

// Protocol version used during handshake.
pub const PROTOCOL_VERSION: u16 = 2;

// All possible message types for TCP clients.
#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Version(u16),
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
    Get(Address),
    Set(Vec<SetRequest>),
    Subscribe(SubscriptionRequest),
    Devices,
    Ping,
    SystemTime,
}

// The corresponding response types for TCP clients.
#[derive(Serialize, Deserialize, Debug)]
pub enum APIResult {
    Get(Value),
    Set,
    Subscribe,
    Devices(Vec<VirtualDeviceConfig>),
    Ping,
    SystemTime(SystemTime),
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
    pub value: Value,
    pub target: SetRequestTarget,
}

// The target of a set request.
#[derive(Serialize, Deserialize, Debug)]
pub enum SetRequestTarget {
    Address(Address),
    Group(String),
}
