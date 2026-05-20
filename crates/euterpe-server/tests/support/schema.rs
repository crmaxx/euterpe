pub fn load_spec() -> serde_json::Value {
    let yaml: serde_yaml::Value =
        serde_yaml::from_str(include_str!("../../../../openapi/openapi.yaml")).unwrap();
    serde_json::to_value(yaml).unwrap()
}

pub fn schema_from_spec(spec: &serde_json::Value, name: &str) -> serde_json::Value {
    let schema = spec
        .pointer(&format!("/components/schemas/{name}"))
        .cloned()
        .unwrap_or_else(|| panic!("schema {name} not found"));
    resolve_refs(spec, &schema)
}

fn resolve_refs(spec: &serde_json::Value, schema: &serde_json::Value) -> serde_json::Value {
    if let Some(ref_path) = schema.get("$ref").and_then(|r| r.as_str()) {
        if let Some(name) = ref_path.strip_prefix("#/components/schemas/") {
            let target = spec
                .pointer(&format!("/components/schemas/{name}"))
                .expect("ref target");
            return resolve_refs(spec, target);
        }
    }

    match schema {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                if k == "$ref" {
                    continue;
                }
                out.insert(k.clone(), resolve_refs(spec, v));
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(|v| resolve_refs(spec, v)).collect())
        }
        _ => schema.clone(),
    }
}

pub fn validate_schema(schema: &serde_json::Value, instance: &serde_json::Value) {
    let validator = jsonschema::validator_for(schema).expect("valid jsonschema");
    if let Err(error) = validator.validate(instance) {
        panic!("schema validation failed: {error}");
    }
}
