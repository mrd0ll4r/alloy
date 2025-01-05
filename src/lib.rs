#![recursion_limit = "256"]

pub mod amqp;
pub mod api;
pub mod config;
pub mod event;
pub mod program;

pub type Address = u16;
pub type OutputValue = u16;

/// Maps a value from one range to another.
pub fn map_range(from_range: (f64, f64), to_range: (f64, f64), s: f64) -> f64 {
    to_range.0 + (s - from_range.0) * (to_range.1 - to_range.0) / (from_range.1 - from_range.0)
}

/// Maps a value from one range to a Value (16-bit unsigned integer).
pub fn map_to_value(from_range: (f64, f64), s: f64) -> OutputValue {
    map_range(from_range, (LOW as f64, HIGH as f64), s) as OutputValue
}

/// Maps a Value (16-bit unsigned integer) to another range.
pub fn map_from_value(to_range: (f64, f64), v: OutputValue) -> f64 {
    map_range((LOW as f64, HIGH as f64), to_range, v as f64)
}

/// Maps a temperature between -40 and +80 degrees to a Value.
pub fn map_temperature_to_value(temp: f64) -> OutputValue {
    map_to_value((-40_f64, 80_f64), temp)
}

/// Maps a Value back to a temperature between -40 and +80 degrees.
pub fn map_value_to_temperature(v: OutputValue) -> f64 {
    map_from_value((-40_f64, 80_f64), v)
}

/// Maps relative humidity (0-100%) to a Value.
pub fn map_relative_humidity_to_value(humidity: f64) -> OutputValue {
    map_to_value((0_f64, 100_f64), humidity)
}

/// Maps a Value to relative humidity (0-100%).
pub fn map_value_to_relative_humidity(v: OutputValue) -> f64 {
    map_from_value((0_f64, 100_f64), v)
}

/// Maps a pressure (in pascal!) between 950 hPa and 1050 hPa to a Value.
pub fn map_pressure_to_value(pressure: f64) -> OutputValue {
    map_to_value((95_000_f64, 105_000_f64), pressure)
}

/// Maps a Value back to a pressure between 950 hPa and 1050 hPa, in pascal.
pub fn map_value_to_pressure(v: OutputValue) -> f64 {
    map_from_value((95_000_f64, 105_000_f64), v)
}

/// Value generated by _input_ devices on logic high.
pub const HIGH: OutputValue = u16::MAX;
/// Value generated by _input_ devices on logic low.
pub const LOW: OutputValue = u16::MIN;
