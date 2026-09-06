use std::collections::BTreeSet;

use super::{storage::load_revision, *};
use crate::models::ContentDiffChange;

pub async fn diff_content(
    state: &AppState,
    id: Uuid,
    base_revision: i64,
    target_revision: Option<i64>,
) -> Result<ContentDiffV2, ApiError> {
    if base_revision < 1 || target_revision.is_some_and(|value| value < 1) {
        return Err(ApiError::bad_request("Revision numbers must be positive."));
    }
    let current = get_content(state, id).await?;
    let pool = require_postgres(state)?;
    let mut transaction = pool.begin().await?;
    let base = load_revision(&mut transaction, id, base_revision).await?;
    let (target, target_draft_version) = match target_revision {
        Some(revision) => (
            load_revision(&mut transaction, id, revision)
                .await?
                .document,
            None,
        ),
        None => (current.draft.clone(), Some(current.draft.draft_version)),
    };
    transaction.rollback().await?;
    let before = serde_json::to_value(base.document)
        .map_err(|_| ApiError::internal("Content diff serialization failed."))?;
    let after = serde_json::to_value(target)
        .map_err(|_| ApiError::internal("Content diff serialization failed."))?;
    let mut changes = Vec::new();
    collect_changes("", Some(&before), Some(&after), &mut changes);
    Ok(ContentDiffV2 {
        content_id: id,
        base_revision,
        target_revision,
        target_draft_version,
        changes,
    })
}

fn collect_changes(
    path: &str,
    before: Option<&Value>,
    after: Option<&Value>,
    changes: &mut Vec<ContentDiffChange>,
) {
    if before == after || changes.len() >= 5000 {
        return;
    }
    match (before, after) {
        (Some(Value::Object(left)), Some(Value::Object(right))) => {
            let keys = left
                .keys()
                .chain(right.keys())
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            for key in keys {
                let next = format!("{}/{}", path, escape_pointer(key));
                collect_changes(&next, left.get(key), right.get(key), changes);
            }
        }
        _ => changes.push(ContentDiffChange {
            path: if path.is_empty() {
                "/".into()
            } else {
                path.into()
            },
            before: before.cloned(),
            after: after.cloned(),
        }),
    }
}

fn escape_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn diff_uses_stable_json_pointer_paths() {
        let mut changes = Vec::new();
        collect_changes(
            "",
            Some(&json!({"title": "Before", "a/b": true})),
            Some(&json!({"title": "After", "new": 2})),
            &mut changes,
        );
        let paths = changes
            .iter()
            .map(|change| change.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(paths, vec!["/a~1b", "/new", "/title"]);
    }
}
