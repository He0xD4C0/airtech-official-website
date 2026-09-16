use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{error::ApiError, models::CursorPage};

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 100;
const MAX_CURSOR_BYTES: usize = 2_048;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorQuery {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScopedCursor<T> {
    version: u8,
    scope: String,
    position: T,
}

#[derive(Debug, PartialEq)]
pub enum DecodedCursor<Legacy, Current> {
    Legacy(Legacy),
    Current(Current),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawScopedCursor {
    version: u8,
    scope: String,
    position: Value,
}

pub fn cursor_limit(query: &CursorQuery) -> Result<usize, ApiError> {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(ApiError::bad_request("limit must be between 1 and 100."));
    }
    Ok(limit)
}

pub fn encode_scoped_cursor<T: Serialize>(scope: &str, position: &T) -> Result<String, ApiError> {
    serde_json::to_vec(&ScopedCursor {
        version: 2,
        scope: scope.into(),
        position,
    })
    .map(|value| URL_SAFE_NO_PAD.encode(value))
    .map_err(|_| ApiError::internal("Cursor serialization failed."))
}

pub fn decode_scoped_cursor<T: DeserializeOwned>(scope: &str, value: &str) -> Result<T, ApiError> {
    let cursor = decode_raw_cursor(scope, value)?;
    if cursor.version != 2 {
        return Err(invalid_cursor());
    }
    serde_json::from_value(cursor.position).map_err(|_| invalid_cursor())
}

pub fn decode_scoped_cursor_compat<Legacy, Current>(
    scope: &str,
    value: &str,
) -> Result<DecodedCursor<Legacy, Current>, ApiError>
where
    Legacy: DeserializeOwned,
    Current: DeserializeOwned,
{
    let cursor = decode_raw_cursor(scope, value)?;
    match cursor.version {
        1 => serde_json::from_value(cursor.position)
            .map(DecodedCursor::Legacy)
            .map_err(|_| invalid_cursor()),
        2 => serde_json::from_value(cursor.position)
            .map(DecodedCursor::Current)
            .map_err(|_| invalid_cursor()),
        _ => Err(invalid_cursor()),
    }
}

fn decode_raw_cursor(scope: &str, value: &str) -> Result<RawScopedCursor, ApiError> {
    if value.is_empty() || value.len() > MAX_CURSOR_BYTES {
        return Err(invalid_cursor());
    }
    URL_SAFE_NO_PAD
        .decode(value)
        .ok()
        .and_then(|value| serde_json::from_slice::<RawScopedCursor>(&value).ok())
        .filter(|cursor| cursor.scope == scope)
        .ok_or_else(invalid_cursor)
}

fn invalid_cursor() -> ApiError {
    ApiError::bad_request("cursor is invalid or belongs to another list.")
}

pub fn paginate_by_id<T>(
    scope: &'static str,
    values: Vec<T>,
    query: CursorQuery,
    id: impl Fn(&T) -> Uuid,
) -> Result<CursorPage<T>, ApiError> {
    paginate_by_id_scoped(scope, values, query, id)
}

/// Same contract as [`paginate_by_id`], but the cursor scope may include the
/// active filter and sort signature so a cursor cannot survive a filter change.
pub fn paginate_by_id_scoped<T>(
    scope: &str,
    values: Vec<T>,
    query: CursorQuery,
    id: impl Fn(&T) -> Uuid,
) -> Result<CursorPage<T>, ApiError> {
    let limit = cursor_limit(&query)?;
    let start = if let Some(value) = query.cursor.as_deref() {
        let after_id = decode_scoped_cursor::<Uuid>(scope, value)?;
        values
            .iter()
            .position(|item| id(item) == after_id)
            .map(|position| position + 1)
            .ok_or_else(|| ApiError::bad_request("cursor is invalid or has expired."))?
    } else {
        0
    };
    let mut items: Vec<_> = values.into_iter().skip(start).take(limit + 1).collect();
    let has_more = items.len() > limit;
    items.truncate(limit);
    let next_cursor = if has_more {
        items
            .last()
            .map(|item| encode_scoped_cursor(scope, &id(item)))
            .transpose()?
    } else {
        None
    };
    Ok(CursorPage { items, next_cursor })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_in_order_and_binds_cursor_to_scope() {
        let ids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let first = paginate_by_id(
            "content",
            ids.to_vec(),
            CursorQuery {
                cursor: None,
                limit: Some(2),
            },
            |id| *id,
        )
        .unwrap();
        assert_eq!(first.items, ids[..2]);
        let second = paginate_by_id(
            "content",
            ids.to_vec(),
            CursorQuery {
                cursor: first.next_cursor,
                limit: Some(2),
            },
            |id| *id,
        )
        .unwrap();
        assert_eq!(second.items, ids[2..]);
        assert!(second.next_cursor.is_none());
    }

    #[test]
    fn rejects_cross_scope_and_out_of_range_limits() {
        let id = Uuid::new_v4();
        let cursor = paginate_by_id(
            "content",
            vec![id, Uuid::new_v4()],
            CursorQuery {
                cursor: None,
                limit: Some(1),
            },
            |value| *value,
        )
        .unwrap()
        .next_cursor;
        assert!(paginate_by_id(
            "products",
            vec![id],
            CursorQuery {
                cursor,
                limit: Some(1),
            },
            |value| *value,
        )
        .is_err());
        assert!(paginate_by_id(
            "content",
            vec![id],
            CursorQuery {
                cursor: None,
                limit: Some(0),
            },
            |value| *value,
        )
        .is_err());
    }

    #[test]
    fn generic_cursor_round_trips_keysets_and_rejects_changed_scope() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Position {
            date: String,
            hash: String,
        }

        let position = Position {
            date: "2026-09-02".into(),
            hash: "opaque-dimension-hash".into(),
        };
        let cursor = encode_scoped_cursor("analytics|from=*|to=*", &position).unwrap();
        assert_eq!(
            decode_scoped_cursor::<Position>("analytics|from=*|to=*", &cursor).unwrap(),
            position
        );
        assert!(decode_scoped_cursor::<Position>(
            "analytics|from=2026-09-01T00:00:00Z|to=*",
            &cursor
        )
        .is_err());
    }

    #[test]
    fn compatibility_decoder_distinguishes_v1_and_v2_positions() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Position {
            created_at: String,
            id: Uuid,
        }

        let legacy_id = Uuid::new_v4();
        let legacy = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&ScopedCursor {
                version: 1,
                scope: "admin.test".to_owned(),
                position: legacy_id,
            })
            .unwrap(),
        );
        assert_eq!(
            decode_scoped_cursor_compat::<Uuid, Position>("admin.test", &legacy).unwrap(),
            DecodedCursor::Legacy(legacy_id)
        );
        assert!(decode_scoped_cursor::<Uuid>("admin.test", &legacy).is_err());

        let current_position = Position {
            created_at: "2026-09-16T00:00:00Z".into(),
            id: Uuid::new_v4(),
        };
        let current = encode_scoped_cursor("admin.test", &current_position).unwrap();
        assert_eq!(
            decode_scoped_cursor_compat::<Uuid, Position>("admin.test", &current).unwrap(),
            DecodedCursor::Current(current_position)
        );
    }

    #[test]
    fn strict_decoder_rejects_malformed_and_oversized_values() {
        assert!(decode_scoped_cursor::<Uuid>("admin.test", "not-base64").is_err());
        assert!(
            decode_scoped_cursor::<Uuid>("admin.test", &"x".repeat(MAX_CURSOR_BYTES + 1)).is_err()
        );
    }
}
