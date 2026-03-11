use serde::{Deserialize, Serialize};

/// A FHIR R4 resource stored in the encrypted database.
///
/// Fields use camelCase serialization to match Tauri 2's default
/// serde behavior for frontend interop.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FhirResource {
    pub id: String,
    pub resource_type: String,
    pub resource: serde_json::Value,
    pub version_id: i64,
    pub last_updated: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Input for creating a new FHIR resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFhirResource {
    pub resource_type: String,
    pub resource: serde_json::Value,
}

/// Input for updating an existing FHIR resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFhirResource {
    pub id: String,
    pub resource: serde_json::Value,
}

/// Response wrapper for listing FHIR resources.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FhirResourceList {
    pub resources: Vec<FhirResource>,
    pub total: i64,
}
