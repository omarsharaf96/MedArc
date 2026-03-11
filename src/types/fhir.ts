/**
 * TypeScript types mirroring the Rust FHIR structs.
 *
 * Field names use camelCase to match the Rust structs' #[serde(rename_all = "camelCase")].
 */

/** A FHIR R4 resource stored in the encrypted database. */
export interface FhirResource {
  id: string;
  resourceType: string;
  resource: Record<string, unknown>;
  versionId: number;
  lastUpdated: string;
  createdAt: string;
  updatedAt: string;
}

/** Input for creating a new FHIR resource. */
export interface CreateFhirResource {
  resourceType: string;
  resource: Record<string, unknown>;
}

/** Input for updating an existing FHIR resource. */
export interface UpdateFhirResource {
  id: string;
  resource: Record<string, unknown>;
}

/** Response wrapper for listing FHIR resources. */
export interface FhirResourceList {
  resources: FhirResource[];
  total: number;
}

/** Database health status from the check_db command. */
export interface DbStatus {
  encrypted: boolean;
  cipher_version: string;
  page_count: number;
}

/** Application info from the get_app_info command. */
export interface AppInfo {
  version: string;
  db_path: string;
}
