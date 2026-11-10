use crate::pet::{Action, Pet, Progression};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub pet: Option<Pet>,
    pub progression: Option<Progression>,
    pub events: Vec<String>,
    pub message: Option<String>,
    pub server_time: i64,
}

#[derive(Serialize, Deserialize)]
pub struct StatusSummary {
    pub summary: String,
    pub model: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HatchRequest {
    pub name: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CareRequest {
    pub action: Action,
}

#[derive(Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}
