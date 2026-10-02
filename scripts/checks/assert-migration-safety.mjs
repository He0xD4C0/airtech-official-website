import { existsSync, readFileSync, readdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = dirname(dirname(dirname(fileURLToPath(import.meta.url))))

// Migrations V0001-V0028 were reviewed and released before this rule existed.
// They stay immutable; every later migration must declare destructive intent.
export const GRANDFATHERED_MIGRATIONS = new Set([
  'V0001__platform_foundation.sql',
  'V0002__public_rate_limits.sql',
  'V0003__product_publish_gate.sql',
  'V0004__platform_settings_revision.sql',
  'V0005__operational_data_architecture.sql',
  'V0006__editorial_working_drafts.sql',
  'V0007__product_presentation_revisions.sql',
  'V0008__operational_integrity_hardening.sql',
  'V0009__guest_engagement_materialization.sql',
  'V0010__product_source_takeover.sql',
  'V0011__unified_content_revision_service.sql',
  'V0012__media_upload_pipeline.sql',
  'V0013__direct_public_media.sql',
  'V0014__cms_publication_dependencies.sql',
  'V0015__admin_workflow.sql',
  'V0016__retire_general_operations.sql',
  'V0017__private_content_drafts.sql',
  'V0018__remove_content_history.sql',
  'V0019__cms_query_indexes.sql',
  'V0020__database_object_storage_settings.sql',
  'V0021__media_preview_derivatives.sql',
  'V0022__feishu_automatic_sync.sql',
  'V0023__feishu_source_assets.sql',
  'V0024__feishu_revision_provenance.sql',
  'V0025__feishu_gui_credentials.sql',
  'V0026__feishu_configurable_sources.sql',
  'V0027__feishu_source_reconciliation.sql',
  'V0028__product_source_metadata.sql',
])

export const DESTRUCTIVE_STATEMENTS = [
  ['DROP TABLE', /\bDROP\s+TABLE\b/iu],
  ['DROP COLUMN', /\bDROP\s+COLUMN\b/iu],
  ['DELETE FROM', /\bDELETE\s+FROM\b/iu],
  ['TRUNCATE', /\bTRUNCATE\b/iu],
  ['ALTER COLUMN TYPE', /\bALTER\s+COLUMN\s+\S+\s+(?:SET\s+DATA\s+)?TYPE\b/iu],
]

export const DESTRUCTIVE_MARKER = /^--[ \t]*airtek:destructive:[ \t]*\S+/mu

export function auditMigrationSafety(entries) {
  const failures = []
  for (const entry of entries) {
    if (GRANDFATHERED_MIGRATIONS.has(entry.name)) continue
    const statements = DESTRUCTIVE_STATEMENTS
      .filter(([, pattern]) => pattern.test(entry.body))
      .map(([name]) => name)
    if (statements.length === 0) continue
    if (DESTRUCTIVE_MARKER.test(entry.body)) continue
    failures.push(
      `${entry.name}: destructive statement(s) ${statements.join(', ')} require a '-- airtek:destructive: <reason>' marker.`,
    )
  }
  return failures
}

function loadMigrations() {
  const directory = join(root, 'services/platform/migrations')
  if (!existsSync(directory)) return []
  return readdirSync(directory)
    .filter((name) => /^V\d{4}__.+\.sql$/u.test(name))
    .sort()
    .map((name) => ({ name, body: readFileSync(join(directory, name), 'utf8') }))
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const failures = auditMigrationSafety(loadMigrations())
  if (failures.length) {
    console.error(failures.map((failure) => `- ${failure}`).join('\n'))
    process.exit(1)
  }
  console.log('Migration safety check passed: every non-grandfathered migration declares destructive intent.')
}
