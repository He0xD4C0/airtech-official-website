//! Administrator identity and access-management path definitions.

use serde_json::{Map, Value};

use super::super::support::*;

/// Adds user, invitation, role, and session-management endpoints.
pub(super) fn add_paths(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/users",
        "get",
        admin(
            params(
                op(
                    "listAdminUsers",
                    "List management users and role assignments",
                    "adminIdentity",
                    [(
                        "200",
                        json_response("Management users", r("AdminUserRecordPage")),
                    )],
                ),
                admin_pagination_params(),
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/users/{id}",
        "get",
        admin(
            params(
                op(
                    "getAdminUser",
                    "Get one management user",
                    "adminIdentity",
                    [(
                        "200",
                        response_header(
                            json_response("Management user", r("AdminUserRecord")),
                            "ETag",
                            "Current user revision tag",
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
        "/api/admin/v1/users/{id}",
        "patch",
        admin(
            params(
                body(
                    op(
                        "updateAdminUser",
                        "Update a management user, status or role assignments",
                        "adminIdentity",
                        [(
                            "200",
                            response_header(
                                json_response("Management user updated", r("AdminUserRecord")),
                                "ETag",
                                "New user revision tag",
                            ),
                        )],
                    ),
                    r("UpdateAdminUser"),
                ),
                vec![
                    path_param("id", uuid()),
                    if_match_param(),
                    idempotency_param(),
                ],
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/users/{id}/sessions",
        "delete",
        admin(
            params(
                op(
                    "revokeAdminUserSessions",
                    "Revoke all active sessions for one management user",
                    "adminIdentity",
                    [("204", empty_response("User sessions revoked"))],
                ),
                vec![path_param("id", uuid())],
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/user-invitations",
        "get",
        admin(
            op(
                "listUserInvitations",
                "List management user invitations",
                "adminIdentity",
                [(
                    "200",
                    json_response("User invitations", r("UserInvitationPage")),
                )],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/user-invitations",
        "post",
        admin(
            params(
                body(
                    op(
                        "inviteAdminUser",
                        "Create a time-bounded management user invitation",
                        "adminIdentity",
                        [(
                            "201",
                            json_response("User invitation created", r("UserInvitation")),
                        )],
                    ),
                    r("InviteAdminUser"),
                ),
                vec![idempotency_param()],
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/user-invitations/{id}/revoke",
        "post",
        admin(
            params(
                body(
                    op(
                        "revokeUserInvitation",
                        "Revoke a pending management user invitation",
                        "adminIdentity",
                        [("204", empty_response("User invitation revoked"))],
                    ),
                    r("ReasonRequest"),
                ),
                vec![path_param("id", uuid()), idempotency_param()],
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/roles",
        "get",
        admin(
            op(
                "listAdminRoles",
                "List role definitions and their permission matrices",
                "adminIdentity",
                [(
                    "200",
                    json_response("Role definitions", r("AdminRoleRecordPage")),
                )],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/roles/{id}",
        "get",
        admin(
            params(
                op(
                    "getAdminRole",
                    "Get one role definition and permission matrix",
                    "adminIdentity",
                    [(
                        "200",
                        response_header(
                            json_response("Role definition", r("AdminRoleRecord")),
                            "ETag",
                            "Current role revision tag",
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
        "/api/admin/v1/roles/{id}",
        "patch",
        admin(
            params(
                body(
                    op(
                        "updateAdminRole",
                        "Update a role display name or permission matrix",
                        "adminIdentity",
                        [(
                            "200",
                            response_header(
                                json_response("Role definition updated", r("AdminRoleRecord")),
                                "ETag",
                                "New role revision tag",
                            ),
                        )],
                    ),
                    r("UpdateAdminRole"),
                ),
                vec![
                    path_param("id", uuid()),
                    if_match_param(),
                    idempotency_param(),
                ],
            ),
            true,
        ),
    );
}
