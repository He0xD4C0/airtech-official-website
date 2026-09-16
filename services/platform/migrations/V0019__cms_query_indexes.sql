-- SQL pushdown support for private drafts, review work, and current publication.

CREATE INDEX users_manager_idx
    ON users(manager_user_id,id) WHERE manager_user_id IS NOT NULL;

CREATE INDEX cms_drafts_owner_updated_idx
    ON cms_drafts(owner_user_id,updated_at DESC,draft_id DESC);
CREATE INDEX cms_drafts_unassigned_updated_idx
    ON cms_drafts(updated_at DESC,draft_id DESC) WHERE owner_user_id IS NULL;
CREATE INDEX cms_drafts_state_updated_idx
    ON cms_drafts(state,updated_at DESC,draft_id DESC);
CREATE INDEX cms_drafts_content_idx
    ON cms_drafts(content_id,updated_at DESC,draft_id DESC);
CREATE INDEX cms_draft_shares_user_idx
    ON cms_draft_shares(shared_with_user_id,created_at DESC,draft_id);
CREATE INDEX cms_review_queue_submitted_idx
    ON cms_review_queue(submitted_at,draft_id);

CREATE INDEX cms_published_kind_locale_idx
    ON content_entries(kind,locale,id);
CREATE INDEX cms_published_updated_idx
    ON cms_published_content(updated_at DESC,content_id DESC);
CREATE INDEX cms_published_slug_idx
    ON content_entries(locale,slug,kind,id);
CREATE INDEX cms_published_search_idx ON cms_published_content USING gin (
    to_tsvector('english',
        coalesce(document->>'title','') || ' ' ||
        coalesce(document->>'summary','') || ' ' ||
        coalesce(document->>'slug',''))
);

CREATE INDEX cms_current_dependencies_target_content_idx
    ON cms_current_publication_dependencies(target_content_id,source_content_id)
    WHERE target_content_id IS NOT NULL;
CREATE INDEX cms_current_dependencies_target_product_idx
    ON cms_current_publication_dependencies(target_product_id,target_product_revision)
    WHERE target_product_id IS NOT NULL;
CREATE INDEX cms_current_dependencies_target_media_idx
    ON cms_current_publication_dependencies(target_media_asset_id,dependency_kind)
    WHERE target_media_asset_id IS NOT NULL;

CREATE INDEX users_status_name_idx
    ON users(status,lower(display_name),id);
CREATE INDEX products_admin_updated_idx
    ON products(updated_at DESC,id DESC);
CREATE INDEX products_admin_stable_idx
    ON products(stable_id,id);
CREATE INDEX products_admin_status_family_idx
    ON products(status,family,stable_id,id);
CREATE INDEX product_presentation_admin_title_idx
    ON product_presentation_working USING gin (title gin_trgm_ops);
CREATE INDEX product_specs_admin_state_idx
    ON product_specs(product_id,product_revision,fact_state);
CREATE INDEX audit_admin_time_idx
    ON audit_log(occurred_at DESC,id DESC);
CREATE INDEX roles_admin_name_idx
    ON roles(lower(display_name),id);
CREATE INDEX user_invitations_admin_time_idx
    ON user_invitations(invited_at DESC,id DESC);
CREATE INDEX product_import_runs_admin_time_idx
    ON product_import_runs(created_at DESC,id DESC);
CREATE INDEX product_temporary_overrides_admin_idx
    ON product_temporary_overrides(product_id,created_at DESC,id DESC);
CREATE INDEX sync_runs_admin_time_idx
    ON sync_runs(started_at DESC,id DESC);
CREATE INDEX sync_mappings_admin_order_idx
    ON sync_mappings(active DESC,created_at DESC,id DESC);
CREATE INDEX staging_records_admin_order_idx
    ON staging_records(sync_run_id,validation_status,created_at DESC,id DESC);
CREATE INDEX sync_conflicts_admin_order_idx
    ON sync_conflicts(id DESC) INCLUDE (resolved_at,source_record_id);
CREATE INDEX rfq_submissions_admin_inbox_idx
    ON rfq_submissions(status,assigned_to,updated_at DESC,id DESC);
CREATE INDEX contact_requests_admin_inbox_idx
    ON contact_requests(status,assigned_to,updated_at DESC,id DESC);
