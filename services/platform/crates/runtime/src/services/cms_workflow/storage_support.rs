use crate::auth::AdminPrincipal;

pub(super) const DRAFT_COLUMNS: &str = "draft.draft_id,draft.content_id,\
    draft.owner_user_id,draft.document,draft.draft_version,\
    draft.base_publication_version,draft.state,draft.rejection_reason,\
    draft.created_at,draft.updated_at";

pub(super) fn visibility_sql(user_parameter: usize) -> String {
    format!(
        r#"(
          draft.owner_user_id=${user_parameter}
          OR ${}::boolean
          OR (${}::boolean AND draft.state='pendingReview')
          OR EXISTS (
            SELECT 1 FROM cms_draft_shares share
            JOIN users shared_user ON shared_user.id=share.shared_with_user_id
            WHERE share.draft_id=draft.draft_id
              AND share.shared_with_user_id=${user_parameter}
              AND shared_user.status='active'
          )
          OR draft.owner_user_id IN (
            WITH RECURSIVE reports(id) AS (
              SELECT id FROM users WHERE manager_user_id=${user_parameter}
              UNION ALL
              SELECT users.id FROM users JOIN reports ON users.manager_user_id=reports.id
            ) SELECT id FROM reports
          )
        )"#,
        user_parameter + 1,
        user_parameter + 2
    )
}

pub(super) fn is_super_admin(principal: &AdminPrincipal) -> bool {
    principal.is_super_admin()
}
