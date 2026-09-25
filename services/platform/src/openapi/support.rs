//! Shared OpenAPI document-construction helpers.

use serde_json::{json, Map, Value};

pub(super) fn add(paths: &mut Map<String, Value>, path: &str, method: &str, operation: Value) {
    let path_item = paths
        .entry(path.to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    path_item
        .as_object_mut()
        .expect("path item is an object")
        .insert(method.to_owned(), operation);
}

pub(super) fn admin_pagination_params() -> Vec<Value> {
    vec![
        json!({
            "name": "cursor", "in": "query", "required": false,
            "description": "Opaque endpoint-scoped cursor returned by the previous page.",
            "schema": {"type": "string", "minLength": 1, "maxLength": 2048}
        }),
        json!({
            "name": "limit", "in": "query", "required": false,
            "description": "Page size; values outside 1 through 100 return Problem Details 400.",
            "schema": {"type": "integer", "minimum": 1, "maximum": 100, "default": 50}
        }),
    ]
}

pub(super) fn op<const N: usize>(
    operation_id: &str,
    summary: &str,
    tag: &str,
    successes: [(&str, Value); N],
) -> Value {
    let mut responses: Map<String, Value> = successes
        .into_iter()
        .map(|(status, response)| (status.to_owned(), response))
        .collect();
    for (status, description) in [
        ("400", "Malformed request"),
        ("401", "Admin session required"),
        ("403", "Permission, CSRF, origin, or TOTP check failed"),
        ("404", "Resource or route not found"),
        ("409", "Concurrent or domain conflict"),
        ("422", "Validation failed"),
        ("428", "If-Match precondition required"),
        ("429", "Authentication rate limit exceeded"),
        ("500", "Internal server error"),
        ("503", "Required service is unavailable"),
    ] {
        responses
            .entry(status)
            .or_insert_with(|| problem_response(description));
    }
    json!({"operationId": operation_id, "summary": summary, "tags": [tag], "responses": responses})
}

pub(super) fn body(mut operation: Value, schema: Value) -> Value {
    operation["requestBody"] =
        json!({"required": true, "content": {"application/json": {"schema": schema}}});
    operation
}

pub(super) fn params(mut operation: Value, parameters: Vec<Value>) -> Value {
    operation["parameters"] = Value::Array(parameters);
    operation
}

pub(super) fn admin(mut operation: Value, mutation: bool) -> Value {
    let mut requirement = Map::new();
    requirement.insert("adminSession".into(), json!([]));
    if mutation {
        requirement.insert("csrfToken".into(), json!([]));
    }
    operation["security"] = json!([Value::Object(requirement)]);
    operation
}

pub(super) fn product_entity_op(operation_id: &str, summary: &str) -> Value {
    op(
        operation_id,
        summary,
        "adminCatalog",
        [(
            "200",
            response_header(
                json_response("Product published", r("Product")),
                "ETag",
                "Published revision tag",
            ),
        )],
    )
}

pub(super) fn idempotent_entity_params() -> Vec<Value> {
    vec![
        path_param("id", uuid()),
        if_match_param(),
        idempotency_param(),
    ]
}

pub(super) fn json_response(description: &str, schema: Value) -> Value {
    json!({"description": description, "content": {"application/json": {"schema": schema}}})
}

pub(super) fn text_response(description: &str, content_type: &str, schema: Value) -> Value {
    json!({"description": description, "content": {content_type: {"schema": schema}}})
}

pub(super) fn empty_response(description: &str) -> Value {
    json!({"description": description})
}

pub(super) fn problem_response(description: &str) -> Value {
    json!({"description": description, "content": {"application/problem+json": {"schema": r("ProblemDetails")}}})
}

pub(super) fn with_problem_example(
    mut operation: Value,
    status: &str,
    code: &str,
    title: &str,
    detail: &str,
) -> Value {
    let status_number = status
        .parse::<u16>()
        .expect("OpenAPI problem example status is numeric");
    operation["responses"][status]["content"]["application/problem+json"]["examples"][code] = json!({
        "summary": title,
        "value": {
            "type": crate::error::problem_type_uri(code),
            "title": title,
            "status": status_number,
            "detail": detail,
            "requestId": "00000000-0000-4000-8000-000000000000"
        }
    });
    operation
}

pub(super) fn session_response(description: &str) -> Value {
    let mut response = json_response(description, r("SessionUser"));
    response["headers"] = json!({
        "X-CSRF-Token": {"description": "Fresh CSRF token mirrored in the readable host-only CSRF cookie.", "schema": {"type": "string"}},
        "Set-Cookie": {"description": "Host-only session and CSRF cookies.", "schema": {"type": "string"}}
    });
    response
}

pub(super) fn response_header(mut response: Value, name: &str, description: &str) -> Value {
    response["headers"][name] = json!({"description": description, "schema": {"type": "string"}});
    response
}

pub(super) fn path_param(name: &str, schema: Value) -> Value {
    json!({"name": name, "in": "path", "required": true, "schema": schema})
}

pub(super) fn query_param(name: &str, required: bool, schema: Value) -> Value {
    json!({"name": name, "in": "query", "required": required, "schema": schema})
}

pub(super) fn locale_param(required: bool) -> Value {
    query_param(
        "locale",
        required,
        json!({"type": "string", "enum": ["en"], "default": "en"}),
    )
}

pub(super) fn idempotency_param() -> Value {
    json!({"name": "Idempotency-Key", "in": "header", "required": true, "description": "Replay key scoped to this mutation. Reusing it with the same request returns the original status and entity; a different request returns 409.", "schema": {"type": "string", "minLength": 8, "maxLength": 200}})
}

pub(super) fn if_match_param() -> Value {
    json!({"name": "If-Match", "in": "header", "required": true, "description": "Current entity ETag, formatted as revision-N.", "schema": {"type": "string", "pattern": "^\\\"revision-[0-9]+\\\"$"}})
}

pub(super) fn seo_properties() -> Value {
    json!({
        "title": nullable(json!({"type": "string"})), "description": nullable(json!({"type": "string"})),
        "canonicalPath": nullable(json!({"type": "string"})), "indexable": {"type": "boolean", "default": false}
    })
}

pub(super) fn object(required: &[&str], properties: Value) -> Value {
    json!({"type": "object", "additionalProperties": false, "required": required, "properties": properties})
}

pub(super) fn page(item: &str) -> Value {
    object(
        &["items", "nextCursor"],
        json!({"items": array(r(item)), "nextCursor": nullable(json!({"type": "string"}))}),
    )
}

pub(super) fn product_page() -> Value {
    object(
        &[
            "items",
            "nextCursor",
            "total",
            "familyCounts",
            "motorTechnologyCounts",
        ],
        json!({
            "items": array(r("Product")),
            "nextCursor": {
                "description": "Opaque base64url v3 keyset cursor bound to normalized query and filters; v2 is accepted only when q is absent for one compatibility release.",
                "anyOf": [
                    {"type": "string", "minLength": 1, "maxLength": 2048, "pattern": "^[A-Za-z0-9_-]+$"},
                    {"type": "null"}
                ]
            },
            "total": {"type": "integer", "minimum": 0},
            "familyCounts": array(r("ProductFacetCount")),
            "motorTechnologyCounts": array(r("ProductFacetCount"))
        }),
    )
}

pub(super) fn array(items: Value) -> Value {
    json!({"type": "array", "items": items})
}

pub(super) fn string_enum(values: &[&str]) -> Value {
    json!({"type": "string", "enum": values})
}

pub(super) fn r(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
}

pub(super) fn nullable(schema: Value) -> Value {
    json!({"anyOf": [schema, {"type": "null"}]})
}

pub(super) fn uuid() -> Value {
    json!({"type": "string", "format": "uuid"})
}

pub(super) fn timestamp() -> Value {
    json!({"type": "string", "format": "date-time"})
}

pub(super) fn revision() -> Value {
    json!({"type": "integer", "format": "int64", "minimum": 1})
}

pub(super) fn counter() -> Value {
    json!({"type": "integer", "minimum": 0})
}

pub(super) fn slug() -> Value {
    json!({"type": "string", "minLength": 1, "maxLength": 180, "pattern": "^[a-z0-9]+(?:-[a-z0-9]+)*$"})
}
