//! OpenAPI schemas for administrator-managed outbound integration settings.

use serde_json::{json, Map, Value};

use super::super::support::*;

pub(super) fn add(s: &mut Map<String, Value>) {
    s.insert(
        "MailSettings".into(),
        object(
            &[
                "configured",
                "host",
                "port",
                "protocol",
                "username",
                "fromAddress",
                "fromName",
                "revision",
            ],
            json!({
                "configured": {"type": "boolean"},
                "host": {"type": "string"},
                "port": {"type": "integer"},
                "protocol": {"type": "string", "enum": ["starttls", "tls"]},
                "username": {"type": "string"},
                "fromAddress": {"type": "string"},
                "fromName": {"type": "string"},
                "revision": revision()
            }),
        ),
    );
    s.insert(
        "UpdateMailSettings".into(),
        object(
            &["host", "port", "protocol", "fromAddress", "fromName", "reason"],
            json!({
                "host": {"type": "string", "minLength": 1, "maxLength": 255},
                "port": {"type": "integer", "minimum": 1, "maximum": 65535},
                "protocol": {"type": "string", "enum": ["starttls", "tls"]},
                "username": nullable(json!({"type": "string", "maxLength": 320})),
                "password": nullable(json!({"type": "string", "format": "password", "maxLength": 2048, "writeOnly": true})),
                "fromAddress": {"type": "string", "format": "email"},
                "fromName": {"type": "string", "maxLength": 200},
                "reason": {"type": "string", "minLength": 10}
            }),
        ),
    );
    s.insert(
        "SmsSettings".into(),
        object(
            &[
                "configured",
                "provider",
                "accessKeyId",
                "signName",
                "templateCode",
                "region",
                "revision",
            ],
            json!({
                "configured": {"type": "boolean"},
                "provider": {"type": "string", "const": "aliyun"},
                "accessKeyId": {"type": "string"},
                "signName": {"type": "string"},
                "templateCode": {"type": "string"},
                "region": {"type": "string"},
                "revision": revision()
            }),
        ),
    );
    s.insert(
        "UpdateSmsSettings".into(),
        object(
            &["provider", "signName", "templateCode", "region", "reason"],
            json!({
                "provider": {"type": "string", "const": "aliyun"},
                "accessKeyId": nullable(json!({"type": "string", "maxLength": 512})),
                "accessKeySecret": nullable(json!({"type": "string", "format": "password", "maxLength": 2048, "writeOnly": true})),
                "signName": {"type": "string", "minLength": 1, "maxLength": 200},
                "templateCode": {"type": "string", "minLength": 1, "maxLength": 200},
                "region": {"type": "string", "minLength": 1, "maxLength": 100},
                "reason": {"type": "string", "minLength": 10}
            }),
        ),
    );
    s.insert(
        "CaptchaSettings".into(),
        object(
            &["configured", "provider", "siteKey", "revision"],
            json!({
                "configured": {"type": "boolean"},
                "provider": {"type": "string", "enum": ["turnstile", "recaptcha", "hcaptcha"]},
                "siteKey": {"type": "string"},
                "revision": revision()
            }),
        ),
    );
    s.insert(
        "UpdateCaptchaSettings".into(),
        object(
            &["provider", "siteKey", "reason"],
            json!({
                "provider": {"type": "string", "enum": ["turnstile", "recaptcha", "hcaptcha"]},
                "siteKey": {"type": "string", "minLength": 1, "maxLength": 512},
                "secretKey": nullable(json!({"type": "string", "format": "password", "maxLength": 2048, "writeOnly": true})),
                "reason": {"type": "string", "minLength": 10}
            }),
        ),
    );
    s.insert(
        "MailTestRequest".into(),
        object(
            &[],
            json!({"to": nullable(json!({"type": "string", "format": "email"}))}),
        ),
    );
    s.insert(
        "SmsTestRequest".into(),
        object(
            &["phone"],
            json!({"phone": {"type": "string", "pattern": "^\\+[1-9][0-9]{7,14}$"}}),
        ),
    );
    s.insert(
        "CaptchaTestRequest".into(),
        object(
            &["token"],
            json!({"token": {"type": "string", "minLength": 1, "writeOnly": true}}),
        ),
    );
    s.insert(
        "IntegrationTestResult".into(),
        object(
            &["delivered", "verified"],
            json!({
                "delivered": {"type": "boolean"},
                "verified": {"type": "boolean"}
            }),
        ),
    );
}
