//! Administrator authentication path definitions.

use serde_json::{Map, Value};

use super::super::support::*;

/// Adds administrator authentication and session endpoints.
pub(super) fn add_paths(paths: &mut Map<String, Value>) {
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
        "/api/admin/v1/auth/password",
        "post",
        admin(
            body(
                op(
                    "changeAdministratorPassword",
                    "Change the current administrator password and revoke other sessions",
                    "adminAuth",
                    [("204", empty_response("Password changed"))],
                ),
                r("ChangePasswordRequest"),
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/auth/identify",
        "post",
        body(
            op(
                "identifyAdministrator",
                "Start a multi-step administrator sign-in without revealing whether the account exists",
                "adminAuth",
                [(
                    "200",
                    json_response("Sign-in methods and CAPTCHA requirement", r("IdentifyResponse")),
                )],
            ),
            r("IdentifyRequest"),
        ),
    );
    add(
        paths,
        "/api/admin/v1/auth/attempt",
        "post",
        body(
            op(
                "attemptAdministratorSignIn",
                "Verify the password or send an email or SMS verification code",
                "adminAuth",
                [(
                    "202",
                    json_response("Next sign-in step", r("LoginStepResponse")),
                )],
            ),
            r("AttemptRequest"),
        ),
    );
    add(
        paths,
        "/api/admin/v1/auth/verify",
        "post",
        body(
            op(
                "verifyAdministratorSignIn",
                "Complete a sign-in factor; the final factor returns the admin session",
                "adminAuth",
                [
                    ("200", session_response("Authenticated admin session")),
                    (
                        "202",
                        json_response("Another factor is required", r("LoginStepResponse")),
                    ),
                ],
            ),
            r("VerifyRequest"),
        ),
    );
    add(
        paths,
        "/api/admin/v1/auth/recovery",
        "post",
        body(
            op(
                "recoverAdministratorWithKey",
                "Reset the root administrator with the offline recovery key and rotate it",
                "adminAuth",
                [(
                    "200",
                    json_response("Rotated recovery key", r("RecoveryKeyRotationResult")),
                )],
            ),
            r("RecoveryRequest"),
        ),
    );
    add(
        paths,
        "/api/admin/v1/auth/recovery-key",
        "get",
        admin(
            op(
                "getAdministratorRecoveryKey",
                "Read recovery-key state; plaintext is returned only before first confirmation",
                "adminAuth",
                [(
                    "200",
                    json_response("Recovery key state", r("RecoveryKeyState")),
                )],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/auth/recovery-key/confirm",
        "post",
        admin(
            op(
                "confirmAdministratorRecoveryKey",
                "Record that the administrator stored the recovery key offline",
                "adminAuth",
                [("204", empty_response("Recovery key confirmed"))],
            ),
            true,
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
        "/api/admin/v1/auth/phone/verification",
        "post",
        admin(
            body(
                op(
                    "startAdministratorPhoneVerification",
                    "Send a binding code to a new phone number after verifying the current password",
                    "adminAuth",
                    [(
                        "202",
                        json_response(
                            "Binding code sent",
                            r("PhoneVerificationStarted"),
                        ),
                    )],
                ),
                r("StartPhoneVerificationRequest"),
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/auth/phone/confirm",
        "post",
        admin(
            body(
                op(
                    "confirmAdministratorPhoneVerification",
                    "Confirm the binding code and mark the phone number verified",
                    "adminAuth",
                    [("204", empty_response("Phone number verified"))],
                ),
                r("ConfirmPhoneVerificationRequest"),
            ),
            true,
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
                    "Verify the enrollment code and enable TOTP for the account",
                    "adminAuth",
                    [("204", empty_response("TOTP enabled"))],
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
