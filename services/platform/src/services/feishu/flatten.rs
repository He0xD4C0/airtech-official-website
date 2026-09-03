fn flatten(value: &Value) -> BTreeMap<String, Value> {
    let mut output = BTreeMap::new();
    flatten_at("", value, &mut output);
    output
}

fn flatten_at(path: &str, value: &Value, output: &mut BTreeMap<String, Value>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                flatten_at(&child_path, child, output);
            }
        }
        _ => {
            output.insert(path.to_owned(), value.clone());
        }
    }
}
