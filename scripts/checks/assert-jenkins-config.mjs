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
requireMatch(ci, /^\s+workflow_dispatch:\s*$/mu, 'GitHub CI must remain available through manual dispatch.')
forbidMatch(ci, /^  (?:push|pull_request):/mu, 'GitHub CI automatic triggers must remain disabled during Jenkins migration.')

const pipeline = read('Jenkinsfile')
requireMatch(pipeline, /disableConcurrentBuilds/u, 'Jenkins must prevent concurrent builds of one branch.')
requireMatch(pipeline, /stage\('CI'\)[\s\S]*parallel/u, 'Jenkins must run isolated CI workspaces in parallel.')
for (const script of ['frontend.sh', 'rust.sh', 'browser.sh']) {
  requireMatch(pipeline, new RegExp(`scripts/ci/${script.replace('.', '\\.')}`, 'u'), `Jenkins must execute ${script}.`)
}
requireMatch(pipeline, /when\s*\{\s*branch 'cicd'\s*\}/u, 'Only cicd may enter the image-publishing stage.')
requireMatch(pipeline, /credentialsId: 'airtek-ghcr-push'/u, 'Jenkins must use the dedicated GHCR credential ID.')
requireMatch(pipeline, /lock\(resource: 'airtek-ghcr-publish'\)/u, 'GHCR publishing must use a global lock.')
requireMatch(pipeline, /env\.CD_ENABLED == 'true'/u, 'Deployment must require the explicit CD_ENABLED switch.')
requireMatch(pipeline, /PUBLISHED_NOT_DEPLOYED/u, 'Disabled CD must report PUBLISHED_NOT_DEPLOYED.')
forbidMatch(pipeline, /docker login[\s\S]*stage\('CI'\)/u, 'Image registry authentication must not precede CI.')

const casc = read('infra/jenkins/jenkins.yaml')
requireMatch(casc, /numExecutors:\s*0/u, 'The Jenkins controller must have zero executors.')
requireMatch(casc, /name:\s*"airtek-builder"[\s\S]*numExecutors:\s*3/u, 'JCasC must define the independent local build agent.')
requireMatch(casc, /key:\s*"CD_ENABLED"\s*\n\s*value:\s*"false"/u, 'CD must default to disabled in JCasC.')
requireMatch(casc, /includes\('main cicd'\)/u, 'The multibranch job must only discover main and cicd.')
requireMatch(casc, /credentialsId\('airtek-github-read'\)/u, 'Private repository polling must use its dedicated read-only credential ID.')
forbidMatch(casc, /airtek-ghcr-push[\s\S]*(?:password|secret):/u, 'GHCR credentials must not be serialized in JCasC.')

const publish = read('scripts/ci/publish-images.sh')
requireMatch(publish, /docker buildx bake[\s\S]*--push/u, 'cicd publishing must use Buildx and push images.')
requireMatch(publish, /trivy image[\s\S]*cyclonedx/u, 'Publishing must generate CycloneDX SBOM files.')
requireMatch(publish, /vulnerabilities\.json/u, 'Publishing must archive per-image vulnerability reports.')
requireMatch(publish, /trivy image[^\n]+\|\| true/u, 'Trivy findings must be report-only for this rollout.')

const deploy = read('scripts/ci/deploy-release.sh')
requireMatch(deploy, /StrictHostKeyChecking=yes/u, 'CD must enforce strict SSH host-key checking.')
forbidMatch(deploy, /production\.env/u, 'Daily CD must not transfer the production environment file.')
forbidMatch(deploy, /docker compose[^\n]*down|DROP DATABASE|flyway[^\n]*undo/iu, 'Daily CD must not destroy infrastructure or reverse migrations.')

const publicDockerfile = read('infra/docker/Dockerfile.web')
const adminDockerfile = read('infra/docker/Dockerfile.admin')
forbidMatch(publicDockerfile, /VITE_PUBLIC_(?:ORIGIN|API_BASE_URL)/u, 'Public production images must be domain-neutral.')
forbidMatch(adminDockerfile, /VITE_ADMIN_API_BASE_URL/u, 'Admin production images must be domain-neutral.')
requireMatch(adminDockerfile, /40-airtek-runtime-config\.sh/u, 'Admin must generate runtime config during container startup.')
const compose = read('infra/compose/production.app.yaml')
requireMatch(compose, /PUBLIC_API_BROWSER_ORIGIN:\s*\$\{AIRTEK_API_ORIGIN:\?/u, 'Production Public must receive the runtime API origin.')
requireMatch(compose, /PUBLIC_ORIGIN:\s*\$\{AIRTEK_PUBLIC_ORIGIN:\?/u, 'Production Public must receive its canonical runtime origin.')
requireMatch(compose, /ADMIN_API_ORIGIN:\s*\$\{AIRTEK_API_ORIGIN:\?/u, 'Production Admin must receive the runtime API origin.')

if (failures.length) {
  console.error(failures.map((failure) => `- ${failure}`).join('\n'))
  process.exit(1)
}
console.log('Jenkins, branch promotion, immutable publishing and runtime-origin boundaries are valid.')
