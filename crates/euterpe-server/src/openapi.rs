use serde_json::Value;

const OPENAPI_YAML: &str = include_str!("../../../openapi/openapi.yaml");

pub fn spec_json() -> Result<Value, serde_json::Error> {
    let yaml: serde_yaml::Value =
        serde_yaml::from_str(OPENAPI_YAML).expect("embedded openapi.yaml must parse");
    serde_json::to_value(yaml)
}
