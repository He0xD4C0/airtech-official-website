//! Administrator News and General Information path definitions.

use serde_json::{Map, Value};

use super::super::support::*;

/// Adds editorial News and General Information endpoints.
pub(super) fn add_paths(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/news",
        "get",
        admin(
            params(
                op(
                    "listAdminNews",
                    "List News working records",
                    "adminNews",
                    [("200", json_response("News records", r("NewsPage")))],
                ),
                admin_pagination_params(),
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/news",
        "post",
        admin(
            params(
                body(
                    op(
                        "createNewsDraft",
                        "Create a News draft",
                        "adminNews",
                        [(
                            "201",
                            response_header(
                                json_response("News draft created", r("NewsEntry")),
                                "ETag",
                                "Current working revision tag",
                            ),
                        )],
                    ),
                    r("NewsDraftInput"),
                ),
                vec![idempotency_param()],
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/news/{id}",
        "get",
        admin(
            params(
                op(
                    "getAdminNews",
                    "Get one News working record",
                    "adminNews",
                    [(
                        "200",
                        response_header(
                            json_response("News working record", r("NewsEntry")),
                            "ETag",
                            "Current working revision tag",
                        ),
                    )],
                ),
                vec![path_param("id", uuid())],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/news/{id}",
        "patch",
        admin(
            params(
                body(
                    op(
                        "updateNewsDraft",
                        "Update a News draft",
                        "adminNews",
                        [(
                            "200",
                            response_header(
                                json_response("News draft updated", r("NewsEntry")),
                                "ETag",
                                "New working revision tag",
                            ),
                        )],
                    ),
                    r("NewsDraftInput"),
                ),
                idempotent_entity_params(),
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/news/{id}/revisions",
        "get",
        admin(
            params(
                op(
                    "listNewsRevisions",
                    "List immutable News revision snapshots",
                    "adminNews",
                    [(
                        "200",
                        json_response("News revision snapshots", r("NewsRevisionPage")),
                    )],
                ),
                vec![path_param("id", uuid())],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/news/{id}/publish",
        "post",
        admin(
            params(
                op(
                    "publishNewsRevision",
                    "Publish the current immutable News revision",
                    "adminNews",
                    [(
                        "200",
                        response_header(
                            json_response("News revision published", r("NewsEntry")),
                            "ETag",
                            "Published revision tag",
                        ),
                    )],
                ),
                idempotent_entity_params(),
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/news/{id}/rollback",
        "post",
        admin(
            params(
                body(
                    op(
                        "rollbackNewsRevision",
                        "Republish a historical News revision as a new revision",
                        "adminNews",
                        [(
                            "200",
                            response_header(
                                json_response("News revision republished", r("NewsEntry")),
                                "ETag",
                                "New published revision tag",
                            ),
                        )],
                    ),
                    r("RevisionRequest"),
                ),
                idempotent_entity_params(),
            ),
            true,
        ),
    );

    add(
        paths,
        "/api/admin/v1/general-information",
        "get",
        admin(
            params(
                op(
                    "getGeneralInformation",
                    "Get the locale-aware General Information working record",
                    "adminGeneralInformation",
                    [(
                        "200",
                        response_header(
                            json_response("General Information", r("GeneralInformation")),
                            "ETag",
                            "Current working revision tag",
                        ),
                    )],
                ),
                vec![locale_param(false)],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/general-information",
        "post",
        admin(
            params(
                body(
                    op(
                        "createGeneralInformation",
                        "Create a locale-aware General Information draft",
                        "adminGeneralInformation",
                        [(
                            "201",
                            response_header(
                                json_response(
                                    "General Information draft created",
                                    r("GeneralInformation"),
                                ),
                                "ETag",
                                "Current working revision tag",
                            ),
                        )],
                    ),
                    r("GeneralInformationDraftInput"),
                ),
                vec![idempotency_param()],
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/general-information/{id}",
        "get",
        admin(
            params(
                op(
                    "getGeneralInformationById",
                    "Get one General Information working record",
                    "adminGeneralInformation",
                    [(
                        "200",
                        response_header(
                            json_response("General Information", r("GeneralInformation")),
                            "ETag",
                            "Current working revision tag",
                        ),
                    )],
                ),
                vec![path_param("id", uuid())],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/general-information/{id}",
        "patch",
        admin(
            params(
                body(
                    op(
                        "updateGeneralInformation",
                        "Update a General Information draft",
                        "adminGeneralInformation",
                        [(
                            "200",
                            response_header(
                                json_response(
                                    "General Information draft updated",
                                    r("GeneralInformation"),
                                ),
                                "ETag",
                                "New working revision tag",
                            ),
                        )],
                    ),
                    r("GeneralInformationDraftInput"),
                ),
                idempotent_entity_params(),
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/general-information/{id}/revisions",
        "get",
        admin(
            params(
                op(
                    "listGeneralInformationRevisions",
                    "List immutable General Information revision snapshots",
                    "adminGeneralInformation",
                    [(
                        "200",
                        json_response(
                            "General Information revision snapshots",
                            r("GeneralInformationRevisionPage"),
                        ),
                    )],
                ),
                vec![path_param("id", uuid())],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/general-information/{id}/publish",
        "post",
        admin(
            params(
                op(
                    "publishGeneralInformationRevision",
                    "Publish the current General Information revision",
                    "adminGeneralInformation",
                    [(
                        "200",
                        response_header(
                            json_response("General Information published", r("GeneralInformation")),
                            "ETag",
                            "Published revision tag",
                        ),
                    )],
                ),
                idempotent_entity_params(),
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/general-information/{id}/rollback",
        "post",
        admin(
            params(
                body(
                    op(
                        "rollbackGeneralInformationRevision",
                        "Republish a historical General Information revision",
                        "adminGeneralInformation",
                        [(
                            "200",
                            response_header(
                                json_response(
                                    "General Information revision republished",
                                    r("GeneralInformation"),
                                ),
                                "ETag",
                                "New published revision tag",
                            ),
                        )],
                    ),
                    r("RevisionRequest"),
                ),
                idempotent_entity_params(),
            ),
            true,
        ),
    );
}
