use crate::config::InputValue;
use anyhow::Error;
use serde::{Deserialize, Serialize};
use std::backtrace::BacktraceStatus;
use std::fmt::{Display, Formatter};
use std::time::Duration;

#[derive(Serialize, Deserialize, Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum ButtonEvent {
    /// The button was pressed.
    Down,

    /// The button was depressed.
    Up,

    /// The button was pressed and released.
    /// The duration indicates for how long the button was pressed.
    Clicked { duration: Duration },

    /// An event that is generated at one-second intervals after a button was initially pressed.
    LongPress { seconds: u64 },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AddressedEvent {
    pub alias: String,
    pub event: Event,
}

/// An error, deconstructed into the high-level error message, a chain of causes, and a backtrace.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EventError {
    pub message: String,
    pub chain: Vec<String>,
    pub backtrace: Option<String>,
}

impl Display for EventError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<&Error> for EventError {
    fn from(err: &Error) -> Self {
        let msg = err.to_string();
        let chain = err.chain().skip(1).map(|cause| cause.to_string()).collect();
        let backtrace = match err.backtrace().status() {
            BacktraceStatus::Captured => Some(format!("{:?}", err.backtrace())),
            BacktraceStatus::Unsupported | BacktraceStatus::Disabled | _ => None,
        };

        EventError {
            message: msg,
            chain,
            backtrace,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Event {
    /// The time at which the event was generated, on the generating machine.
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// The actual event.
    pub inner: Result<EventKind, EventError>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum EventKind {
    /// An event generated by buttons and other binary input devices.
    Button(ButtonEvent),

    /// A newly read or written value.
    Update { new_value: InputValue },
}
