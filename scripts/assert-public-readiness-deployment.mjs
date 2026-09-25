import { readFileSync } from 'node:fs'

const compose = readFileSync(new URL('../compose.production.yaml', import.meta.url), 'utf8')
const deploy = readFileSync(new URL('../infra/deploy/deploy-app.sh', import.meta.url), 'utf8')
const failures = []

function serviceBlock(name) {
  const start = compose.search(new RegExp(`^  ${name}:\\s*$`, 'mu'))
  if (start < 0) return ''
  const remainder = compose.slice(start)
  const next = remainder.slice(1).search(/^  [a-z0-9-]+:\s*$/mu)
  return next < 0 ? remainder : remainder.slice(0, next + 1)
}

function requireMatch(body, pattern, detail) {
  if (!pattern.test(body)) failures.push(detail)
}

function forbidMatch(body, pattern, detail) {
  if (pattern.test(body)) failures.push(detail)
}

const readiness = serviceBlock('public-readiness')
requireMatch(readiness, /entrypoint:\s*\["\/usr\/local\/bin\/airtek-maintenance"\]/u, 'Readiness must use the reviewed maintenance binary.')
requireMatch(readiness, /command:\s*\["check-public-readiness"\]/u, 'Readiness must run the complete public-site gate.')
requireMatch(readiness, /gateway:[\s\S]*condition:\s*service_healthy/u, 'Readiness must wait for a healthy gateway.')
for (const variable of [
  'DATABASE_URL',
  'AIRTEK_PUBLIC_ORIGIN',
  'AIRTEK_ADMIN_ORIGIN',
  'AIRTEK_API_ORIGIN',
  'PUBLIC_HOST',
  'ADMIN_HOST',
  'API_HOST',
]) {
  requireMatch(readiness, new RegExp(`${variable}:\\s*\\$\\{${variable}:\\?`, 'u'), `Readiness must require ${variable}.`)
}
forbidMatch(readiness, /AIRTEK_READINESS_ALLOW_HTTP/u, 'Production readiness must never permit plaintext public origins.')
requireMatch(deploy, /run --rm --no-deps public-readiness/u, 'Deployment must pass readiness before promotion.')
requireMatch(deploy, /public-readiness[\s\S]*ln -sfn "\$release_dir" "\$deploy_root\/current"/u, 'Readiness must run before the release symlink is promoted.')

if (failures.length) {
  console.error(failures.map((failure) => `- ${failure}`).join('\n'))
  process.exit(1)
}
console.log('Public-site production readiness deployment checks passed.')
