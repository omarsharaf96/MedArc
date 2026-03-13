/// biometric.rs — Touch ID availability check and reason string.
///
/// The macOS implementation calls `LAContext::canEvaluatePolicy_error` to
/// check whether Touch ID (biometric) authentication is available on the
/// current device. The non-macOS fallback always returns `false`.

#[cfg(target_os = "macos")]
mod macos {
    use objc2::rc::Retained;
    use objc2_local_authentication::{LAContext, LAPolicy};

    /// Check if biometric authentication (Touch ID) is available on this device.
    ///
    /// Creates a temporary `LAContext`, calls `canEvaluatePolicy_error` for
    /// `DeviceOwnerAuthenticationWithBiometrics`, and returns `true` iff the
    /// policy can be evaluated (i.e., Touch ID hardware is present and enrolled).
    pub fn check_biometric_available() -> bool {
        // SAFETY: LAContext is an ObjC object. We create it, use it, and
        // drop it within this function on the same thread. No cross-thread
        // sharing occurs.
        unsafe {
            let ctx: Retained<LAContext> = LAContext::new();
            ctx.canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthenticationWithBiometrics)
                .is_ok()
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod fallback {
    /// On non-macOS platforms biometrics are never available.
    pub fn check_biometric_available() -> bool {
        false
    }
}

/// Check if biometric authentication (Touch ID) is available on current hardware.
///
/// Returns `true` on macOS when Touch ID hardware is present and enrolled.
/// Returns `false` on all other platforms or when Touch ID is unavailable.
#[cfg(target_os = "macos")]
pub fn check_biometric_available() -> bool {
    macos::check_biometric_available()
}

#[cfg(not(target_os = "macos"))]
pub fn check_biometric_available() -> bool {
    fallback::check_biometric_available()
}

/// Return the reason string for the LAContext biometric prompt.
pub fn authenticate_biometric_reason() -> String {
    "Unlock MedArc session".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that check_biometric_available() is callable and returns a bool
    /// on any platform. The actual value depends on hardware/OS; we only verify
    /// the return type and that it doesn't panic.
    #[test]
    fn biometric_check_available_returns_bool() {
        let result: bool = check_biometric_available();
        // On CI/non-Touch-ID hardware this will be false; on enrolled hardware true.
        // We just confirm the function is callable and type-correct.
        let _ = result;
    }

    #[test]
    fn authenticate_biometric_reason_is_non_empty() {
        let reason = authenticate_biometric_reason();
        assert!(!reason.is_empty(), "reason string should not be empty");
        assert_eq!(reason, "Unlock MedArc session");
    }
}
