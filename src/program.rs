use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KaleidoscopeMetadata {
    pub fixtures: HashMap<String, FixtureMetadata>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FixtureMetadata {
    pub programs: HashMap<String, ProgramMetadata>,
    pub selected_program: String,
    pub output_aliases: HashSet<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProgramPerformanceMetrics {
    pub runtime_last_ms: f64,
    pub runtime_last_100_mean_ms: f64,
    pub runtime_last_100_sd_ms: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProgramMetadata {
    pub parameters: HashMap<String, ProgramParameter>,
    //pub last_executed: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProgramParameter {
    #[serde(flatten)]
    pub inner: ParameterType,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ParameterType {
    #[serde(rename = "discrete")]
    Discrete {
        levels: HashMap<String, DiscreteParameterLevel>,
        current_level: String,
    },
    #[serde(rename = "continuous")]
    Continuous {
        lower_limit_incl: f64,
        upper_limit_incl: f64,
        current: f64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiscreteParameterLevel {
    pub description: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ParameterSetRequest {
    #[serde(rename = "discrete")]
    Discrete { level: String },
    #[serde(rename = "continuous")]
    Continuous { value: f64 },
}
