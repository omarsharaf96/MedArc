/// Check if biometric authentication (Touch ID) is available on current hardware.
///
/// Note: tauri-plugin-biometry is not included in this build -- Touch ID is a
/// convenience feature that gracefully degrades to unavailable. The actual
/// biometric authentication call happens through the Tauri plugin system from
/// the frontend when the plugin is available.
pub fn check_biometric_available() -> bool {
    // Without tauri-plugin-biometry, biometrics are not available.
    // When the plugin is added in a future iteration, this will use
    // the plugin's availability check.
    false
}

/// Return the reason string for the LAContext biometric prompt.
pub fn authenticate_biometric_reason() -> String {
    "Unlock MedArc session".to_string()
}
