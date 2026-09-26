import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs'
import { join, relative, resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')
const roots = [
  'compose.yaml',
  'compose.debug.yaml',
  'compose.production.yaml',
  '.env.example',
  'infra/docker',
  'infra/gateway',
  'infra/object-storage',
  'infra/deploy/production.env.example',
  '.github/workflows/ci.yml',
  'scripts/run-e2e-stack.mjs',
  'apps/admin/src',
  'apps/web/src',
  'services/platform/crates',
  'services/platform/apps',
  'packages/contracts/openapi',
  'packages/contracts/src/generated',
]
const forbidden = /clamav|clamd|media[_-]?scanner|malware|scanStatus|scan_status|mediaMalwareScan|mediaObjectVerify|mediaImageInspect|mediaDisplayBuild|mediaUploadReconcile|AIRTEK_MEDIA_V2_PUBLIC_ENABLED|AIRTEK_MEDIA_REQUIRED/iu
const forbiddenTaskFiles = /(?:image_pipeline|inventory|upload_reconcile|scanner|review|backfill)/iu
const failures = []

for (const entry of roots) {
  const path = join(root, entry)
  if (!existsSync(path)) {
    failures.push(`${entry} is missing.`)
    continue
  }
  for (const file of files(path)) {
    const name = relative(root, file)
    if (forbiddenTaskFiles.test(name) && name.includes('services/platform/crates/jobs/src/worker/')) {
      failures.push(`${name} is an obsolete media Worker component.`)
    }
    const lines = readFileSync(file, 'utf8').split(/\r?\n/u)
    lines.forEach((line, index) => {
      if (forbidden.test(line)) failures.push(`${name}:${index + 1} contains an obsolete media pipeline term.`)
    })
  }
}

if (failures.length) {
  console.error(failures.map((failure) => `- ${failure}`).join('\n'))
  process.exit(1)
}

console.log('Direct media boundary checks passed.')

function files(path) {
  if (!statSync(path).isDirectory()) return [path]
  return readdirSync(path, { withFileTypes: true }).flatMap((entry) => {
    const child = join(path, entry.name)
    return entry.isDirectory() ? files(child) : [child]
  })
}
