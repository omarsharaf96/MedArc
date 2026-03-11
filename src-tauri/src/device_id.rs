/// device_id.rs — Device identifier managed state.
///
/// Provides a stable, hardware-derived identifier for audit log entries.
/// Uses the `machine-uid` crate to read the OS-native machine ID without
/// requiring root/admin privileges (reads /etc/machine-id on Linux,
/// IOPlatformUUID on macOS, MachineGuid registry key on Windows).
///
/// Falls back to the sentinel "DEVICE_UNKNOWN" if the OS cannot supply a
/// machine ID (e.g. some sandbox environments). This is always logged at
/// app startup so the operator knows which device_id appears in audit rows.

/// Managed Tauri state that holds the device identifier.
///
/// Registered via `app.manage(DeviceId::from_machine_uid())` in `lib.rs`.
/// All audit writes use the value stored here.
#[derive(Debug, Clone)]
pub struct DeviceId {
    id: String,
}

impl DeviceId {
    /// Create a new `DeviceId` with the given identifier string.
    pub fn new(id: impl Into<String>) -> Self {
        DeviceId { id: id.into() }
    }

    /// Attempt to read the hardware machine UID from the OS.
    ///
    /// On success, returns the OS-native UUID/hex string (stable across reboots).
    /// On failure, returns a `DeviceId` carrying `"DEVICE_UNKNOWN"` and logs a
    /// warning so the operator can investigate.
    pub fn from_machine_uid() -> Self {
        match machine_uid::get() {
            Ok(uid) => {
                let trimmed = uid.trim().to_string();
                if trimmed.is_empty() {
                    eprintln!(
                        "[MedArc] WARNING: machine-uid returned an empty string; \
                         audit rows will carry 'DEVICE_UNKNOWN'"
                    );
                    DeviceId {
                        id: "DEVICE_UNKNOWN".to_string(),
                    }
                } else {
                    eprintln!("[MedArc] INFO: device_id resolved to '{}'", trimmed);
                    DeviceId { id: trimmed }
                }
            }
            Err(e) => {
                eprintln!(
                    "[MedArc] WARNING: could not resolve machine-uid ({}); \
                     audit rows will carry 'DEVICE_UNKNOWN'",
                    e
                );
                DeviceId {
                    id: "DEVICE_UNKNOWN".to_string(),
                }
            }
        }
    }

    /// Return the placeholder used before machine-uid is wired.
    ///
    /// Kept for backwards-compat with tests that don't need a real machine UID.
    #[allow(dead_code)]
    pub fn placeholder() -> Self {
        DeviceId {
            id: "DEVICE_PENDING".to_string(),
        }
    }

    /// Get the device identifier string.
    pub fn get(&self) -> &str {
        &self.id
    }

    /// Alias for `get()` — returns the device identifier string.
    pub fn id(&self) -> &str {
        &self.id
    }
}
