-- Release 2 of the login refactor. This migration is destructive on purpose:
-- the deployment stops the gateway and application containers before running
-- it, and the migration artifact retains a pre-migration dump so a failed
-- application rollout can restore the exact previous database.
--
-- airtek:destructive: one-time recovery codes are replaced by the Super Admin reset flow and the offline root recovery key.

DROP TABLE recovery_codes;

-- Every stored TOTP enrollment is retired so administrators re-enroll under the
-- optional-TOTP model. Ciphertext was sealed with the deployment key, so this
-- also removes any dependence on that key being present at startup.
UPDATE users
SET totp_secret_ciphertext = NULL,
    totp_confirmed_at = NULL,
    updated_at = now()
WHERE totp_secret_ciphertext IS NOT NULL OR totp_confirmed_at IS NOT NULL;

-- airtek:destructive: legacy lockouts and challenge counters predate the new login flow and would block valid administrators after the upgrade.
DELETE FROM auth_rate_limits;
