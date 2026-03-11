use serde_json::Value;

/// Filter a FHIR resource JSON to include only allowed fields.
///
/// If `allowed_fields` contains "*", the resource is returned unmodified (full access).
/// Otherwise, only top-level keys present in `allowed_fields` are kept.
/// Non-object values (arrays, strings, etc.) pass through unchanged.
pub fn filter_resource(resource: &Value, allowed_fields: &[&str]) -> Value {
    // Wildcard means full access -- return everything
    if allowed_fields.contains(&"*") {
        return resource.clone();
    }

    match resource {
        Value::Object(map) => {
            let filtered: serde_json::Map<String, Value> = map
                .iter()
                .filter(|(key, _)| allowed_fields.contains(&key.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            Value::Object(filtered)
        }
        // Non-object values pass through unchanged
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn wildcard_returns_full_resource() {
        let resource = json!({
            "id": "123",
            "name": "John",
            "clinicalNotes": "sensitive data",
            "birthDate": "1990-01-01"
        });
        let result = filter_resource(&resource, &["*"]);
        assert_eq!(result, resource);
    }

    #[test]
    fn filter_strips_non_allowed_fields() {
        let resource = json!({
            "id": "123",
            "name": "John",
            "clinicalNotes": "sensitive data",
            "birthDate": "1990-01-01",
            "medications": ["aspirin"]
        });
        let result = filter_resource(&resource, &["id", "name"]);
        let expected = json!({
            "id": "123",
            "name": "John"
        });
        assert_eq!(result, expected);
    }

    #[test]
    fn filter_empty_allowed_returns_empty_object() {
        let resource = json!({
            "id": "123",
            "name": "John"
        });
        let result = filter_resource(&resource, &[]);
        assert_eq!(result, json!({}));
    }

    #[test]
    fn non_object_passthrough_array() {
        let resource = json!([1, 2, 3]);
        let result = filter_resource(&resource, &["id"]);
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn non_object_passthrough_string() {
        let resource = json!("hello");
        let result = filter_resource(&resource, &["id"]);
        assert_eq!(result, json!("hello"));
    }

    #[test]
    fn non_object_passthrough_null() {
        let resource = json!(null);
        let result = filter_resource(&resource, &["id"]);
        assert_eq!(result, json!(null));
    }

    #[test]
    fn filter_preserves_nested_objects() {
        let resource = json!({
            "id": "123",
            "name": [{"given": ["John"], "family": "Doe"}],
            "clinicalNotes": "sensitive",
            "telecom": [{"system": "phone", "value": "555-1234"}]
        });
        let result = filter_resource(&resource, &["id", "name", "telecom"]);
        let expected = json!({
            "id": "123",
            "name": [{"given": ["John"], "family": "Doe"}],
            "telecom": [{"system": "phone", "value": "555-1234"}]
        });
        assert_eq!(result, expected);
    }

    #[test]
    fn filter_billing_staff_patient() {
        let resource = json!({
            "id": "pat-1",
            "name": [{"given": ["Jane"], "family": "Smith"}],
            "birthDate": "1985-03-15",
            "gender": "female",
            "address": [{"city": "Springfield"}],
            "telecom": [{"system": "phone", "value": "555-0001"}],
            "identifier": [{"system": "MRN", "value": "12345"}],
            "contact": [{"name": "Emergency Contact"}],
            "clinicalNotes": "Confidential clinical data",
            "medications": ["metformin"]
        });
        let billing_fields = vec![
            "id",
            "name",
            "birthDate",
            "gender",
            "address",
            "telecom",
            "identifier",
        ];
        let result = filter_resource(&resource, &billing_fields);

        // Should include demographics
        assert!(result.get("id").is_some());
        assert!(result.get("name").is_some());
        assert!(result.get("birthDate").is_some());
        // Should NOT include clinical data
        assert!(result.get("clinicalNotes").is_none());
        assert!(result.get("medications").is_none());
        // Should NOT include contact (billing doesn't get contact)
        assert!(result.get("contact").is_none());
    }
}
