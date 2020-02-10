use serde::{Serialize,Deserialize};

/// Configuration for one virtual device.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VirtualDeviceConfig {
    pub address: u16,
    pub alias: String,
    pub groups: Vec<String>,
    pub mapping: MappingConfig,
    pub read_only: bool,
}

/// Mapping of one virtual device to one port on a hardware device.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MappingConfig {
    pub device: String,
    pub port: u8,
}
