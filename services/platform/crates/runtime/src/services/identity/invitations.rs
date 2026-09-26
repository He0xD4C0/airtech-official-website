use super::*;

pub async fn lock_invitation_email(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    normalized_email: &str,
) -> Result<(), ApiError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "airtek.identity.invitation.email:{normalized_email}"
        ))
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

pub async fn expire_pending_invitation(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    normalized_email: &str,
) -> Result<Option<Uuid>, ApiError> {
    Ok(sqlx::query_scalar(
        r#"UPDATE user_invitations SET status='expired'
           WHERE lower(email)=lower($1) AND status='pending' AND expires_at<=now()
             AND accepted_at IS NULL AND revoked_at IS NULL
           RETURNING id"#,
    )
    .bind(normalized_email)
    .fetch_optional(&mut **transaction)
    .await?)
}

pub async fn invitation_conflicts(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    normalized_email: &str,
) -> Result<(bool, bool), ApiError> {
    let account_exists =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE lower(email)=lower($1))")
            .bind(normalized_email)
            .fetch_one(&mut **transaction)
            .await?;
    let pending_exists = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM user_invitations WHERE lower(email)=lower($1) AND status='pending')",
    )
    .bind(normalized_email)
    .fetch_one(&mut **transaction)
    .await?;
    Ok((account_exists, pending_exists))
}

pub async fn insert_invitation(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    invitation: &UserInvitation,
    token_hash: Vec<u8>,
    invited_by: Uuid,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO user_invitations
           (id,email,display_name,locale,token_hash,invited_by,invited_at,expires_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"#,
    )
    .bind(invitation.id)
    .bind(&invitation.email)
    .bind(&invitation.display_name)
    .bind(&invitation.locale)
    .bind(token_hash)
    .bind(invited_by)
    .bind(invitation.invited_at)
    .bind(invitation.expires_at)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        "INSERT INTO user_invitation_roles(invitation_id,role_id) SELECT $1,id FROM roles WHERE key=ANY($2)",
    )
    .bind(invitation.id)
    .bind(&invitation.role_keys)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub async fn revoke_invitation(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    revoked_by: Uuid,
    reason: &str,
) -> Result<u64, ApiError> {
    Ok(sqlx::query(
        r#"UPDATE user_invitations SET status='revoked',revoked_at=now(),revoked_by=$2,revoke_reason=$3
           WHERE id=$1 AND status='pending' AND expires_at>now()
             AND accepted_at IS NULL AND revoked_at IS NULL"#,
    )
    .bind(id)
    .bind(revoked_by)
    .bind(reason)
    .execute(&mut **transaction)
    .await?
    .rows_affected())
}
