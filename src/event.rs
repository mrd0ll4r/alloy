use crate::config::InputValue;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Serialize, Deserialize, Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum ButtonEvent {
    Down,
    Up,
    Clicked { duration: Duration },
    LongPress { seconds: u64 },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AddressedEvent {
    pub address: u16,
    pub event: Event,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Event {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub inner: Result<EventKind, String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum EventKind {
    Button(ButtonEvent),
    Update { new_value: InputValue },
}

/// A timestamped input value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampedInputValue {
    pub ts: chrono::DateTime<chrono::Utc>,
    pub value: Result<InputValue, String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
pub struct EventFilter {
    pub strategy: EventFilterStrategy,
    pub entries: Vec<EventFilterEntry>,
}

impl EventFilter {
    fn match_inner(entry: &EventFilterEntry, e: &Event) -> bool {
        match entry {
            EventFilterEntry::Any => true,
            EventFilterEntry::Kind { kind } => match &e.inner {
                Ok(inner) => match kind {
                    EventFilterKind::Update => match inner {
                        EventKind::Update { .. } => true,
                        _ => false,
                    },
                    EventFilterKind::Button { filter } => match inner {
                        EventKind::Button(e) => match filter {
                            ButtonEventFilter::Down => match e {
                                ButtonEvent::Down => true,
                                _ => false,
                            },
                            ButtonEventFilter::Up => match e {
                                ButtonEvent::Up => true,
                                _ => false,
                            },
                            ButtonEventFilter::Clicked => match e {
                                ButtonEvent::Clicked { .. } => true,
                                _ => false,
                            },
                            ButtonEventFilter::LongPress => match e {
                                ButtonEvent::LongPress { .. } => true,
                                _ => false,
                            },
                        },
                        _ => false,
                    },
                },
                Err(_) => false,
            },
        }
    }

    pub fn matches(&self, e: &Event) -> bool {
        match &self.strategy {
            EventFilterStrategy::Any => {
                self.entries.iter().any(|entry| Self::match_inner(entry, e))
            }
            EventFilterStrategy::All => {
                self.entries.iter().all(|entry| Self::match_inner(entry, e))
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
pub enum EventFilterStrategy {
    Any,
    All,
}

#[derive(Serialize, Deserialize, Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum EventFilterEntry {
    Any,
    Kind { kind: EventFilterKind },
}

#[derive(Serialize, Deserialize, Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum EventFilterKind {
    Update,
    Button { filter: ButtonEventFilter },
}

#[derive(Serialize, Deserialize, Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
pub enum ButtonEventFilter {
    Down,
    Up,
    Clicked,
    LongPress,
}
