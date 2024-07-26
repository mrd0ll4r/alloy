use crate::config::InputValue;
use crate::event::EventError;
use crate::{Address, OutputValue};
use serde::{Deserialize, Serialize};

/// The target for a set request.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum SetRequestTarget {
    Address(Address),
    Alias(String),
    //Tag(String),
}

/// A request to set one or more outputs to a value.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SetRequest {
    pub value: OutputValue,
    pub target: SetRequestTarget,
}

/// A timestamped input value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampedInputValue {
    pub ts: chrono::DateTime<chrono::Utc>,
    pub value: Result<InputValue, EventError>,
}
