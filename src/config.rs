use crate::{HIGH, LOW};
use serde::{Deserialize, Serialize};

/// Configuration for one virtual device.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VirtualDeviceConfig {
    pub address: u16,
    pub alias: String,
    pub groups: Vec<String>,
    pub mapping: MappingConfig,
    pub read_only: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ValueScaling {
    Linear { from: u16, to: u16 },
    Logarithmic,
}

impl Default for ValueScaling {
    fn default() -> Self {
        ValueScaling::Linear {
            from: LOW,
            to: HIGH,
        }
    }
}

/// Mapping of one virtual device to one port on a hardware device.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MappingConfig {
    pub device: String,
    pub port: u8,
    #[serde(default)]
    pub scaling: Option<ValueScaling>,
}
