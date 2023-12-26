use crate::Address;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UniverseConfig {
    pub version: u64,
    pub devices: Vec<DeviceConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DeviceConfig {
    pub alias: String,
    pub device_type: DeviceType,

    pub inputs: Vec<InputPortConfig>,
    pub outputs: Vec<OutputPortConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DeviceType {
    DHT22,
    DHT22Expander,
    DS18B20Expander,
    PCA9685,
    DS18,
    MCP23017,
    BME280,
    GPIO,
    FanHeater,
    ButtonExpander,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InputPortConfig {
    pub alias: String,
    pub input_type: InputValueType,
    pub tags: HashSet<String>,
    pub port: u8,
    pub address: Address,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum InputValueType {
    Binary,
    Temperature,
    Humidity,
    Pressure,
    Continuous,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputPortConfig {
    pub alias: String,
    pub tags: HashSet<String>,
    pub port: u8,
    pub address: Address,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialOrd, PartialEq)]
pub enum InputValue {
    Binary(bool),
    Temperature(f64),
    Humidity(f64),
    Pressure(f64),
    Continuous(u16),
}
