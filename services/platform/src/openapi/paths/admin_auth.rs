//! Administrator authentication path definitions.

use serde_json::{Map, Value};

use super::super::support::*;

/// Adds administrator authentication and session endpoints.
pub(super) fn add_paths(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/auth/setup",
        "post",
        body(
            op(
                "setupInitialAdministrator",
                "Create the first administrator with a deployment bootstrap token",
                "adminAuth",
                [("201", session_response("Initial administrator created"))],
            ),
            r("SetupRequest"),
        ),
    );
    add(
        paths,
        "/api/admin/v1/auth/login",
        "post",
        body(
            op(
                "loginAdministrator",
                "Create an admin cookie session",
                "adminAuth",
                [("200", session_response("Authenticated admin session"))],
            ),
            r("LoginRequest"),
        ),
    );
    add(
        paths,
        "/api/admin/v1/auth/invitations/accept",
        "post",
        body(
            op(
                "acceptAdministratorInvitation",
                "Activate an invited administrator with a one-time token and strong password",
                "adminAuth",
                [(
                    "201",
                    json_response(
                        "Administrator invitation accepted",
                        r("InvitationAcceptance"),
                    ),
                )],
            ),
            r("AcceptInvitationRequest"),
        ),
    );
    add(
        paths,
        "/api/admin/v1/auth/session",
        "get",
        admin(
            op(
                "getAdminSession",
                "Get the current admin session and rotate CSRF",
                "adminAuth",
                [("200", session_response("Current admin session"))],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/auth/logout",
        "post",
        admin(
            op(
                "logoutAdministrator",
                "Revoke the current admin session",
                "adminAuth",
                [("204", empty_response("Session revoked"))],
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/auth/totp/enrollment",
        "post",
        admin(
            op(
                "startTotpEnrollment",
                "Create or replace an unconfirmed encrypted TOTP enrollment secret",
                "adminAuth",
                [(
                    "200",
                    json_response("TOTP enrollment details", r("TotpEnrollment")),
                )],
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/auth/totp/confirm",
        "post",
        admin(
            body(
                op(
                    "confirmTotpEnrollment",
                    "Verify enrollment and return a one-time recovery-code set",
                    "adminAuth",
                    [(
                        "200",
                        json_response("One-time recovery codes", r("RecoveryCodeSet")),
                    )],
                ),
                r("TotpCodeRequest"),
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/auth/recovery-codes/regenerate",
        "post",
        admin(
            body(
                op(
                    "regenerateRecoveryCodes",
                    "Invalidate prior recovery codes and return a new one-time set",
                    "adminAuth",
                    [(
                        "200",
                        json_response("Replacement recovery codes", r("RecoveryCodeSet")),
                    )],
                ),
                r("TotpCodeRequest"),
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/auth/sessions",
        "get",
        admin(
            op(
                "listAdminSessions",
                "List the current administrator's active sessions",
                "adminAuth",
                [(
                    "200",
                    json_response("Active sessions", array(r("AdminSession"))),
                )],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/auth/sessions/{id}",
        "delete",
        admin(
            params(
                op(
                    "revokeAdminSession",
                    "Revoke one active session owned by the current administrator",
                    "adminAuth",
                    [("204", empty_response("Session revoked"))],
                ),
                vec![path_param("id", uuid())],
            ),
            true,
        ),
    );
}
