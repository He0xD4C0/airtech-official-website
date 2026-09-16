use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    error::ApiError,
    models::{
        AdminRolePage, AdminRoleRecord, AdminUserPage, AdminUserRecord, CursorPage,
        UpdateAdminRole, UpdateAdminUser, UserInvitation,
    },
    pagination::{
        cursor_limit, decode_scoped_cursor, decode_scoped_cursor_compat, encode_scoped_cursor,
        CursorQuery, DecodedCursor,
    },
    services::request_metrics::LegacyCursorEndpoint,
    state::AppState,
};

#[derive(Debug, Deserialize, Serialize)]
struct RoleCursor {
    display_name: String,
    id: Uuid,
}

#[derive(Debug, Deserialize, Serialize)]
struct InvitationCursor {
    invited_at: DateTime<Utc>,
    id: Uuid,
}

pub async fn list_users(
    state: &AppState,
    search: Option<&str>,
    status: Option<&str>,
    pagination: CursorQuery,
) -> Result<AdminUserPage, ApiError> {
    let scope = format!("admin.users|{search:?}|{status:?}");
    let limit = cursor_limit(&pagination)?;
    let after = match pagination.cursor.as_deref() {
        Some(value) => match decode_scoped_cursor_compat::<Uuid, Uuid>(&scope, value)? {
            DecodedCursor::Current(id) => Some(id),
            DecodedCursor::Legacy(id) => {
                state
                    .request_metrics
                    .record_legacy_cursor(LegacyCursorEndpoint::AdminUsers);
                Some(id)
            }
        },
        None => None,
    };
    let total = sqlx::query_scalar::<_, i64>(
        r#"SELECT count(*) FROM users account
           WHERE ($1::text IS NULL OR account.status=$1)
             AND ($2::text IS NULL OR account.display_name ILIKE '%'||$2||'%'
                  OR account.email ILIKE '%'||$2||'%'
                  OR EXISTS (SELECT 1 FROM user_roles assignment
                    JOIN roles role ON role.id=assignment.role_id
                    WHERE assignment.user_id=account.id AND role.key ILIKE '%'||$2||'%'))"#,
    )
    .bind(status)
    .bind(search)
    .fetch_one(&state.pool)
    .await?;
    let rows = sqlx::query(
        r#"SELECT user_account.id,user_account.email,user_account.display_name,
                  user_account.locale,user_account.status,user_account.revision,
                  user_account.manager_user_id,
                  user_account.totp_confirmed_at IS NOT NULL AS totp_enabled,
                  user_account.invited_at,user_account.last_login_at,
                  user_account.created_at,user_account.updated_at,
                  COALESCE(array_agg(role.key ORDER BY role.key)
                    FILTER (WHERE role.key IS NOT NULL),ARRAY[]::text[]) AS roles
           FROM users user_account
           LEFT JOIN user_roles assignment ON assignment.user_id=user_account.id
           LEFT JOIN roles role ON role.id=assignment.role_id
           WHERE ($1::text IS NULL OR user_account.status=$1)
             AND ($2::text IS NULL OR user_account.display_name ILIKE '%'||$2||'%'
                  OR user_account.email ILIKE '%'||$2||'%'
                  OR EXISTS (SELECT 1 FROM user_roles searched_assignment
                    JOIN roles searched_role ON searched_role.id=searched_assignment.role_id
                    WHERE searched_assignment.user_id=user_account.id
                      AND searched_role.key ILIKE '%'||$2||'%'))
             AND ($3::uuid IS NULL OR user_account.id<$3)
           GROUP BY user_account.id ORDER BY user_account.id DESC LIMIT $4"#,
    )
    .bind(status)
    .bind(search)
    .bind(after)
    .bind(i64::try_from(limit + 1).unwrap_or(101))
    .fetch_all(&state.pool)
    .await?;
    let mut users = rows
        .into_iter()
        .map(decode_admin_user)
        .collect::<Result<Vec<_>, _>>()?;
    let has_more = users.len() > limit;
    users.truncate(limit);
    let next_cursor = has_more
        .then(|| users.last())
        .flatten()
        .map(|user| encode_scoped_cursor(&scope, &user.id))
        .transpose()?;
    Ok(AdminUserPage {
        items: users,
        next_cursor,
        total: usize::try_from(total).unwrap_or(usize::MAX),
    })
}

pub async fn list_roles(
    state: &AppState,
    search: Option<&str>,
    pagination: CursorQuery,
) -> Result<AdminRolePage, ApiError> {
    let scope = format!("admin.roles|{search:?}");
    let limit = cursor_limit(&pagination)?;
    let after = match pagination.cursor.as_deref() {
        Some(value) => match decode_scoped_cursor_compat::<Uuid, RoleCursor>(&scope, value)? {
            DecodedCursor::Current(cursor) => Some(cursor),
            DecodedCursor::Legacy(id) => {
                let display_name = sqlx::query_scalar::<_, String>(
                    r#"SELECT lower(role.display_name) FROM roles role WHERE role.id=$1
                       AND ($2::text IS NULL OR role.key ILIKE '%'||$2||'%'
                         OR role.display_name ILIKE '%'||$2||'%'
                         OR EXISTS(SELECT 1 FROM role_permissions permission
                           WHERE permission.role_id=role.id
                             AND permission.permission_key ILIKE '%'||$2||'%'))"#,
                )
                .bind(id)
                .bind(search)
                .fetch_optional(&state.pool)
                .await?
                .ok_or_else(|| ApiError::bad_request("cursor is invalid or has expired."))?;
                state
                    .request_metrics
                    .record_legacy_cursor(LegacyCursorEndpoint::AdminRoles);
                Some(RoleCursor { display_name, id })
            }
        },
        None => None,
    };
    let total = role_count(state, search).await?;
    let rows = sqlx::query(
        r#"SELECT role.id,role.key,role.display_name,role.system_role,role.revision,
                  COALESCE(array_agg(permission.permission_key ORDER BY permission.permission_key)
                    FILTER (WHERE permission.permission_key IS NOT NULL),ARRAY[]::text[]) AS permissions
           FROM roles role LEFT JOIN role_permissions permission ON permission.role_id=role.id
           WHERE ($1::text IS NULL OR role.key ILIKE '%'||$1||'%'
                  OR role.display_name ILIKE '%'||$1||'%'
                  OR EXISTS(SELECT 1 FROM role_permissions searched
                    WHERE searched.role_id=role.id AND searched.permission_key ILIKE '%'||$1||'%'))
             AND ($2::text IS NULL OR (lower(role.display_name),role.id)>($2,$3))
           GROUP BY role.id ORDER BY lower(role.display_name),role.id LIMIT $4"#,
    )
    .bind(search)
    .bind(after.as_ref().map(|value| value.display_name.as_str()))
    .bind(after.as_ref().map(|value| value.id))
    .bind(i64::try_from(limit + 1).unwrap_or(101))
    .fetch_all(&state.pool)
    .await?;
    let mut roles = rows
        .into_iter()
        .map(decode_admin_role)
        .collect::<Result<Vec<_>, _>>()?;
    let has_more = roles.len() > limit;
    roles.truncate(limit);
    let next_cursor = has_more
        .then(|| roles.last())
        .flatten()
        .map(|role| {
            encode_scoped_cursor(
                &scope,
                &RoleCursor {
                    display_name: role.display_name.to_lowercase(),
                    id: role.id,
                },
            )
        })
        .transpose()?;
    Ok(AdminRolePage {
        items: roles,
        next_cursor,
        total: usize::try_from(total).unwrap_or(usize::MAX),
    })
}

pub async fn list_invitations(
    state: &AppState,
    pagination: CursorQuery,
) -> Result<CursorPage<UserInvitation>, ApiError> {
    let scope = "admin.invitations";
    let limit = cursor_limit(&pagination)?;
    let after = pagination
        .cursor
        .as_deref()
        .map(|value| decode_scoped_cursor::<InvitationCursor>(scope, value))
        .transpose()?;
    let rows = sqlx::query(
        r#"SELECT invitation.id,invitation.email,invitation.display_name,
                  invitation.locale,invitation.invited_at,invitation.expires_at,
                  invitation.accepted_at,invitation.revoked_at,
                  COALESCE(array_agg(role.key ORDER BY role.key)
                    FILTER (WHERE role.key IS NOT NULL),ARRAY[]::text[]) AS role_keys
           FROM user_invitations invitation
           LEFT JOIN user_invitation_roles assignment ON assignment.invitation_id=invitation.id
           LEFT JOIN roles role ON role.id=assignment.role_id
           WHERE ($1::timestamptz IS NULL OR (invitation.invited_at,invitation.id)<($1,$2))
           GROUP BY invitation.id ORDER BY invitation.invited_at DESC,invitation.id DESC LIMIT $3"#,
    )
    .bind(after.as_ref().map(|value| value.invited_at))
    .bind(after.as_ref().map(|value| value.id))
    .bind(i64::try_from(limit + 1).unwrap_or(101))
    .fetch_all(&state.pool)
    .await?;
    let mut items = rows
        .into_iter()
        .map(decode_invitation)
        .collect::<Result<Vec<_>, _>>()?;
    let has_more = items.len() > limit;
    items.truncate(limit);
    let next_cursor = has_more
        .then(|| items.last())
        .flatten()
        .map(|item| {
            encode_scoped_cursor(
                scope,
                &InvitationCursor {
                    invited_at: item.invited_at,
                    id: item.id,
                },
            )
        })
        .transpose()?;
    Ok(CursorPage { items, next_cursor })
}

async fn role_count(state: &AppState, search: Option<&str>) -> Result<i64, ApiError> {
    Ok(sqlx::query_scalar(
        r#"SELECT count(*) FROM roles role
           WHERE $1::text IS NULL OR role.key ILIKE '%'||$1||'%'
              OR role.display_name ILIKE '%'||$1||'%'
              OR EXISTS(SELECT 1 FROM role_permissions permission
                WHERE permission.role_id=role.id
                  AND permission.permission_key ILIKE '%'||$1||'%')"#,
    )
    .bind(search)
    .fetch_one(&state.pool)
    .await?)
}

#[path = "identity/storage.rs"]
mod storage;
pub use storage::*;
#[path = "identity/invitations.rs"]
mod invitations;
pub use invitations::*;
