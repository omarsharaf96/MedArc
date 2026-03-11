/**
 * Type-safe wrappers around Tauri invoke() calls.
 *
 * Each function maps to a Rust #[tauri::command] in the backend.
 * Parameter names match the Rust function parameter names exactly.
 */
import { invoke } from "@tauri-apps/api/core";

import type {
  DbStatus,
  AppInfo,
  FhirResource,
  FhirResourceList,
  CreateFhirResource,
  UpdateFhirResource,
} from "../types/fhir";

export const commands = {
  /** Check database encryption health status. */
  checkDb: () => invoke<DbStatus>("check_db"),

  /** Get application version and database path. */
  getAppInfo: () => invoke<AppInfo>("get_app_info"),

  /** Create a new FHIR resource. */
  createResource: (input: CreateFhirResource) =>
    invoke<FhirResource>("create_resource", { input }),

  /** Retrieve a single FHIR resource by ID. */
  getResource: (id: string) => invoke<FhirResource>("get_resource", { id }),

  /** List FHIR resources, optionally filtered by resource type. */
  listResources: (resourceType?: string) =>
    invoke<FhirResourceList>("list_resources", {
      resource_type: resourceType ?? null,
    }),

  /** Update an existing FHIR resource's JSON content. */
  updateResource: (input: UpdateFhirResource) =>
    invoke<FhirResource>("update_resource", { input }),

  /** Delete a FHIR resource by ID. */
  deleteResource: (id: string) => invoke<void>("delete_resource", { id }),
};
