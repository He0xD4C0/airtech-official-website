-- Retire the user-facing general operations capability while preserving history.
DELETE FROM role_permissions
WHERE permission_key = 'operations.run';

DELETE FROM permissions
WHERE key = 'operations.run';

UPDATE jobs
SET status = 'failed',
    last_error = 'This general operations job type was retired.',
    lease_owner = NULL,
    lease_expires_at = NULL,
    updated_at = now()
WHERE status IN ('queued', 'running')
  AND job_type IN (
    'migrationPreflight',
    'migrationApply',
    'backup',
    'restoreValidate',
    'searchReindex',
    'cacheInvalidate',
    'feishuSync'
  );

UPDATE operation_runs
SET status = 'failed',
    result = jsonb_build_object(
      'error', 'This general operations task was retired without execution.'
    ),
    updated_at = now()
WHERE status IN ('queued', 'running')
  AND kind IN (
    'migrationPreflight',
    'migrationApply',
    'backup',
    'restoreValidate',
    'searchReindex',
    'cacheInvalidate',
    'feishuSync'
  );
