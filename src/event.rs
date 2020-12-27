use crate::Value;
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

#[derive(Serialize, Deserialize, Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
pub struct AddressedEvent {
    pub address: u16,
    pub event: Event,
}

#[derive(Serialize, Deserialize, Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
pub struct Event {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub inner: EventKind,
}

#[derive(Serialize, Deserialize, Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
pub enum EventKind {
    Button(ButtonEvent),
    Change { new_value: Value },
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
            EventFilterEntry::Kind { kind } => match kind {
                EventFilterKind::Change => match e.inner {
                    EventKind::Change { .. } => true,
                    _ => false,
                },
                EventFilterKind::Button { filter } => match &e.inner {
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
        }
    }

    pub fn matches(&self, e: &Event) -> bool {
        match &self.strategy {
            EventFilterStrategy::Any => {
                for entry in &self.entries {
                    if Self::match_inner(entry, e) {
                        return true;
                    }
                }
                false
            }
            EventFilterStrategy::All => {
                for entry in &self.entries {
                    if !Self::match_inner(entry, e) {
                        return false;
                    }
                }
                true
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
    Change,
    Button { filter: ButtonEventFilter },
}

#[derive(Serialize, Deserialize, Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
pub enum ButtonEventFilter {
    Down,
    Up,
    Clicked,
    LongPress,
}
