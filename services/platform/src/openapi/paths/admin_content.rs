//! Private draft, review queue, and current published content APIs.

use serde_json::{json, Map, Value};

use super::super::support::*;

pub(super) fn add_paths(paths: &mut Map<String, Value>) {
    add_drafts(paths);
    add_reviews(paths);
    add_published(paths);
    add_media_list(paths);
}

fn add_drafts(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/content-drafts",
        "get",
        admin(
            params(
                op(
                    "listPrivateContentDrafts",
                    "List private drafts visible to the current user",
                    "adminContentDrafts",
                    [("200", json_response("Private drafts", r("CmsDraftPage")))],
                ),
                list_params(),
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/content-drafts",
        "post",
        admin(
            body(
                op(
                    "createPrivateContentDraft",
                    "Create a new private draft",
                    "adminContentDrafts",
                    [("201", draft_response("Private draft created"))],
                ),
                r("ContentDraftV2"),
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/content-drafts/templates",
        "get",
        admin(
            op(
                "listContentDraftTemplates",
                "List controlled CMS templates",
                "adminContentDrafts",
                [(
                    "200",
                    json_response("Templates", r("ContentTemplateDefinitionPage")),
                )],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/content-drafts/{draftId}",
        "get",
        admin(
            params(
                op(
                    "getPrivateContentDraft",
                    "Read a visible private draft",
                    "adminContentDrafts",
                    [("200", draft_response("Private draft"))],
                ),
                vec![path_param("draftId", uuid())],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/content-drafts/{draftId}",
        "patch",
        admin(
            params(
                body(
                    op(
                        "savePrivateContentDraft",
                        "Explicitly save an owned editing draft",
                        "adminContentDrafts",
                        [("200", draft_response("Private draft saved"))],
                    ),
                    r("ContentDraftV2"),
                ),
                vec![path_param("draftId", uuid()), draft_if_match()],
            ),
            true,
        ),
    );
    add_mutation(
        paths,
        "/api/admin/v1/content-drafts/{draftId}/shares",
        "put",
        "setPrivateContentDraftShares",
        "Replace read-only draft shares",
        Some("CmsDraftSharesRequest"),
        "CmsPrivateDraft",
        false,
    );
    add_mutation(
        paths,
        "/api/admin/v1/content-drafts/{draftId}/claim",
        "post",
        "claimUnassignedPrivateContentDraft",
        "Claim an unassigned migrated draft as Super Admin",
        None,
        "CmsPrivateDraft",
        false,
    );
    add_mutation(
        paths,
        "/api/admin/v1/content-drafts/{draftId}/submit",
        "post",
        "submitPrivateContentDraft",
        "Submit a clean saved draft for review or automatic publication",
        None,
        "CmsSubmitResult",
        true,
    );
    add_mutation(
        paths,
        "/api/admin/v1/content-drafts/{draftId}/withdraw",
        "post",
        "withdrawPrivateContentDraft",
        "Withdraw an owned pending draft",
        None,
        "CmsPrivateDraft",
        false,
    );
}

fn add_reviews(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/content-reviews",
        "get",
        admin(
            params(
                op(
                    "listContentReviews",
                    "List the current review queue",
                    "adminContentReviews",
                    [("200", json_response("Review queue", r("CmsReviewPage")))],
                ),
                list_params(),
            ),
            false,
        ),
    );
    add_mutation(
        paths,
        "/api/admin/v1/content-reviews/{draftId}/approve",
        "post",
        "approveContentReview",
        "Atomically overwrite current publication and delete the draft",
        None,
        "CmsPublishResult",
        false,
    );
    add_mutation(
        paths,
        "/api/admin/v1/content-reviews/{draftId}/reject",
        "post",
        "rejectContentReview",
        "Return a draft to editing with its current rejection reason",
        Some("CmsRejectRequest"),
        "CmsPrivateDraft",
        false,
    );
}

fn add_published(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/published-content",
        "get",
        admin(
            params(
                op(
                    "listCurrentPublishedContent",
                    "List company current published content",
                    "adminPublishedContent",
                    [(
                        "200",
                        json_response("Published content", r("CmsPublishedPage")),
                    )],
                ),
                list_params(),
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/published-content/{contentId}",
        "get",
        admin(
            params(
                op(
                    "getCurrentPublishedContent",
                    "Read company current published content",
                    "adminPublishedContent",
                    [(
                        "200",
                        json_response("Published content", r("CmsPublishedContent")),
                    )],
                ),
                vec![path_param("contentId", uuid())],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/published-content/{contentId}/drafts",
        "post",
        admin(
            params(
                op(
                    "copyPublishedContentToPrivateDraft",
                    "Copy current published content to the user's new private draft",
                    "adminPublishedContent",
                    [("201", draft_response("Private draft copied"))],
                ),
                vec![path_param("contentId", uuid())],
            ),
            true,
        ),
    );
}

fn add_media_list(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/media/assets",
        "get",
        admin(
            params(
                op(
                    "listAdminMediaAssets",
                    "List media assets for the private draft editor",
                    "adminContentDrafts",
                    [("200", json_response("Media assets", r("MediaAssetPage")))],
                ),
                list_params(),
            ),
            false,
        ),
    );
}

#[allow(clippy::too_many_arguments)]
fn add_mutation(
    paths: &mut Map<String, Value>,
    path: &str,
    method: &str,
    operation_id: &str,
    summary: &str,
    request_schema: Option<&str>,
    response_schema: &str,
    if_match: bool,
) {
    let operation = op(
        operation_id,
        summary,
        "adminContentDrafts",
        [("200", json_response("Mutation result", r(response_schema)))],
    );
    let operation = request_schema.map_or(operation.clone(), |schema| body(operation, r(schema)));
    let mut parameters = vec![path_param("draftId", uuid())];
    if if_match {
        parameters.push(draft_if_match());
    }
    add(
        paths,
        path,
        method,
        admin(params(operation, parameters), true),
    );
}

fn list_params() -> Vec<Value> {
    let mut values = admin_pagination_params();
    values.push(query_param(
        "q",
        false,
        json!({"type":"string","maxLength":200}),
    ));
    values
}

fn draft_if_match() -> Value {
    json!({
        "name":"If-Match","in":"header","required":true,
        "schema":{"type":"string","pattern":"^\\\"draft-[0-9]+\\\"$"}
    })
}

fn draft_response(description: &str) -> Value {
    response_header(
        json_response(description, r("CmsPrivateDraft")),
        "ETag",
        "Current draft-N entity tag",
    )
}
