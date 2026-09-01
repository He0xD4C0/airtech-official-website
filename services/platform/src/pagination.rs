use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
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
struct Cursor {
    version: u8,
    scope: String,
    after_id: Uuid,
}

pub fn paginate_by_id<T>(
    scope: &'static str,
    values: Vec<T>,
    query: CursorQuery,
    id: impl Fn(&T) -> Uuid,
) -> Result<CursorPage<T>, ApiError> {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(ApiError::bad_request("limit must be between 1 and 100."));
    }
    let start = if let Some(value) = query.cursor.as_deref() {
        let after_id = decode_cursor(scope, value)?;
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
            .map(|item| encode_cursor(scope, id(item)))
            .transpose()?
    } else {
        None
    };
    Ok(CursorPage { items, next_cursor })
}

fn encode_cursor(scope: &str, after_id: Uuid) -> Result<String, ApiError> {
    serde_json::to_vec(&Cursor {
        version: 1,
        scope: scope.into(),
        after_id,
    })
    .map(|value| URL_SAFE_NO_PAD.encode(value))
    .map_err(|_| ApiError::internal("Cursor serialization failed."))
}

fn decode_cursor(scope: &str, value: &str) -> Result<Uuid, ApiError> {
    if value.is_empty() || value.len() > MAX_CURSOR_BYTES {
        return Err(ApiError::bad_request("cursor is invalid."));
    }
    URL_SAFE_NO_PAD
        .decode(value)
        .ok()
        .and_then(|value| serde_json::from_slice::<Cursor>(&value).ok())
        .filter(|cursor| cursor.version == 1 && cursor.scope == scope)
        .map(|cursor| cursor.after_id)
        .ok_or_else(|| ApiError::bad_request("cursor is invalid or belongs to another list."))
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
}
