import { existsSync, readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = dirname(dirname(dirname(fileURLToPath(import.meta.url))))
const failures = []

function read(path) {
  const absolute = join(root, path)
  if (!existsSync(absolute)) {
    failures.push(`${path} is missing.`)
    return ''
  }
  return readFileSync(absolute, 'utf8')
}

function requireMatch(body, pattern, failure) {
  if (!pattern.test(body)) failures.push(failure)
}

function forbidMatch(body, pattern, failure) {
  if (pattern.test(body)) failures.push(failure)
}

const ci = read('.github/workflows/ci.yml')
requireMatch(ci, /^on:\s*\n\s+push:\s*\n\s+tags:\s*\n\s+- '\*'$/mu, 'CI must trigger only when a Git tag is pushed.')
forbidMatch(ci, /^\s+(?:workflow_dispatch|workflow_run|pull_request|schedule):/mu, 'Tag publishing must not have a second workflow trigger.')
requireMatch(ci, /RELEASE_TAG:\s*\$\{\{ github\.ref_name \}\}/u, 'The Git tag must be the release identifier.')
requireMatch(ci, /valid OCI image tag/u, 'The workflow must validate the Git tag before publishing.')
requireMatch(ci, /mutable tag 'latest'/u, 'The Git tag name latest must remain reserved for the mutable image alias.')
requireMatch(ci, /^  publish-images:\s*$/mu, 'The workflow must contain a dedicated image-publishing job.')
for (const prerequisite of ['release-metadata', 'rust-msrv', 'contracts', 'frontend', 'browser-contracts', 'rust', 'deployment']) {
  requireMatch(ci, new RegExp(`^\\s+- ${prerequisite}$`, 'mu'), `Image publishing must wait for ${prerequisite}.`)
}
requireMatch(ci, /^\s+packages:\s+write$/mu, 'Only the publishing job may request GHCR package-write permission.')
requireMatch(ci, /registry:\s+ghcr\.io/u, 'Images must publish to GHCR.')
requireMatch(ci, /password:\s*\$\{\{ github\.token \}\}/u, 'GHCR login must use the short-lived GitHub workflow token.')
requireMatch(ci, /tags:\s*\|[\s\S]*:\$\{\{ needs\.release-metadata\.outputs\.release_tag \}\}[\s\S]*:\s*latest/u, 'Every image must publish both the Git tag and latest alias.')
requireMatch(ci, /^  prune-images:\s*$/mu, 'The workflow must contain a GHCR retention job.')
requireMatch(ci, /KEEP_RELEASE_VERSIONS:\s*'5'/u, 'GHCR retention must keep the five newest release images.')
requireMatch(ci, /run:\s*bash scripts\/maintenance\/prune-ghcr-versions\.sh/u, 'GHCR retention must use the repository pruning script.')
requireMatch(ci, /org\.opencontainers\.image\.revision=\$\{\{ github\.sha \}\}/u, 'Published images must retain their source commit SHA as metadata.')
requireMatch(ci, /provenance:\s+mode=max/u, 'Published images must include build provenance.')
requireMatch(ci, /sbom:\s+true/u, 'Published images must include an SBOM attestation.')
for (const image of ['public-web', 'admin-web', 'platform', 'migrations', 'gateway']) {
  requireMatch(ci, new RegExp(`- image: ${image}`, 'u'), `The ${image} image is missing from the publishing matrix.`)
}
forbidMatch(ci, /sshagent|appleboy\/ssh|SSH_PRIVATE_KEY|deploy_after_publish/u, 'CI must not contain target-server deployment credentials or actions.')

for (const retiredPath of [
  'Jenkinsfile',
  '.github/workflows/release-production.yml',
  'infra/jenkins/jenkins.yaml',
  'scripts/ci/deploy-release.sh',
]) {
  if (existsSync(join(root, retiredPath))) failures.push(`${retiredPath} must remain retired.`)
}

const publicDockerfile = read('infra/docker/Dockerfile.web')
const adminDockerfile = read('infra/docker/Dockerfile.admin')
forbidMatch(publicDockerfile, /VITE_PUBLIC_(?:ORIGIN|API_BASE_URL)/u, 'Public production images must remain domain-neutral.')
forbidMatch(adminDockerfile, /VITE_ADMIN_API_BASE_URL/u, 'Admin production images must remain domain-neutral.')
requireMatch(adminDockerfile, /40-airtek-runtime-config\.sh/u, 'Admin must generate runtime configuration during container startup.')

const compose = read('infra/compose/production.app.yaml')
requireMatch(compose, /PUBLIC_API_BROWSER_ORIGIN:\s*\$\{AIRTEK_API_ORIGIN:\?/u, 'Production Public must receive the runtime API origin.')
requireMatch(compose, /PUBLIC_ORIGIN:\s*\$\{AIRTEK_PUBLIC_ORIGIN:\?/u, 'Production Public must receive its canonical runtime origin.')
requireMatch(compose, /ADMIN_API_ORIGIN:\s*\$\{AIRTEK_API_ORIGIN:\?/u, 'Production Admin must receive the runtime API origin.')

const deploy = read('infra/deploy/deploy-app.sh')
requireMatch(deploy, /release_tag=\$\{1:-\}/u, 'The target-server script must accept the Git tag as its release identifier.')
requireMatch(deploy, /AIRTEK_PLATFORM_IMAGE=\$image_prefix-platform:\$release_tag/u, 'The target-server script must pull the tag-published platform image.')

const prune = read('scripts/maintenance/prune-ghcr-versions.sh')
requireMatch(prune, /select\(any\(\.metadata\.container\.tags\[\]\?; \. != "latest"\)\)/u, 'Retention must count version-tagged images instead of untagged OCI artifacts.')
requireMatch(prune, /sort_by\(\.created_at\)/u, 'Retention must order release images by creation time.')
requireMatch(prune, /gh api --method DELETE/u, 'Retention must delete release versions outside the keep set.')

if (failures.length) {
  console.error(failures.map((failure) => `- ${failure}`).join('\n'))
  process.exit(1)
}

console.log('Git-tag CI, GHCR publishing and manual deployment boundaries are valid.')
