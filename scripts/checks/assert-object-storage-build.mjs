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

const minioDockerfile = read('infra/docker/Dockerfile.minio')
const mcDockerfile = read('infra/docker/Dockerfile.mc')
const compose = read('compose.yaml')
const media = read('infra/compose/media.yaml')
const localEnvironment = read('.env.example')

requireMatch(minioDockerfile, /^ARG\s+MINIO_TAG=RELEASE\.2025-09-07T16-13-09Z$/mu, 'MinIO image must pin the reviewed upstream release tag.')
requireMatch(minioDockerfile, /^ARG\s+MINIO_COMMIT=07c3a429bfed433e49018cb0f78a52145d4bedeb$/mu, 'MinIO image must pin the reviewed upstream commit.')
requireMatch(minioDockerfile, /git -C \/src rev-parse HEAD/u, 'MinIO image must verify the checked-out commit before building.')
requireMatch(minioDockerfile, /apt-get install[^\n]*curl/u, 'MinIO image must install curl for container health probes.')
requireMatch(minioDockerfile, /^USER\s+10001$/mu, 'MinIO image must run as the non-root object-storage UID.')
requireMatch(minioDockerfile, /COPY --from=build \/out\/minio/u, 'MinIO image must copy the compiled server binary.')
requireMatch(minioDockerfile, /minio-LICENSE/u, 'MinIO image must bundle the upstream license.')
requireMatch(minioDockerfile, /mount=type=cache,target=\/go\/pkg\/mod/u, 'MinIO build must cache downloaded Go modules.')

requireMatch(mcDockerfile, /^ARG\s+MC_TAG=RELEASE\.2025-08-13T08-35-41Z$/mu, 'mc image must pin the reviewed upstream release tag.')
requireMatch(mcDockerfile, /^ARG\s+MC_COMMIT=7394ce0dd2a80935aded936b09fa12cbb3cb8096$/mu, 'mc image must pin the reviewed upstream commit.')
requireMatch(mcDockerfile, /git -C \/src rev-parse HEAD/u, 'mc image must verify the checked-out commit before building.')
requireMatch(mcDockerfile, /^USER\s+10001$/mu, 'mc image must run as the non-root object-storage UID.')
requireMatch(mcDockerfile, /COPY --from=build \/out\/mc/u, 'mc image must copy the compiled client binary.')
requireMatch(mcDockerfile, /mc-LICENSE/u, 'mc image must bundle the upstream license.')
requireMatch(mcDockerfile, /mount=type=cache,target=\/go\/pkg\/mod/u, 'mc build must cache downloaded Go modules.')
forbidMatch(mcDockerfile, /FROM\s+(?:scratch|gcr\.io\/distroless)/iu, 'mc image must keep a POSIX shell for the bucket-initialization script.')
forbidMatch(minioDockerfile, /GOTOOLCHAIN=auto/iu, 'Object-storage builds must not download an unpinned Go toolchain.')
forbidMatch(mcDockerfile, /GOTOOLCHAIN=auto/iu, 'Object-storage builds must not download an unpinned Go toolchain.')

for (const [body, label] of [[compose, 'Local Compose'], [media, 'Media Compose']]) {
  requireMatch(body, /\$\{AIRTEK_OBJECT_STORE_IMAGE:\?/u, `${label} must require the configured MinIO server image.`)
}
requireMatch(compose, /\$\{AIRTEK_OBJECT_STORE_MC_IMAGE:\?/u, 'Local Compose must require the configured mc client image.')
requireMatch(media, /^name:\s*\$\{AIRTEK_MEDIA_PROJECT_NAME:-airtek-media\}$/mu, 'Media Compose must keep the independent airtek-media project name.')
requireMatch(media, /^networks:[\s\S]*external:\s*true/mu, 'Media Compose must join the external production network instead of creating one.')

for (const variable of ['AIRTEK_OBJECT_STORE_IMAGE', 'AIRTEK_OBJECT_STORE_MC_IMAGE', 'AIRTEK_GOPROXY']) {
  requireMatch(localEnvironment, new RegExp(`^${variable}=`, 'mu'), `.env.example must document ${variable}.`)
}

// MinIO withdrew its public images and binaries; a regression to those sources
// would silently break every environment, so the references are forbidden.
for (const path of ['.env.example', 'compose.yaml', 'infra/compose/media.yaml', 'scripts/testing/contract-object-store.mjs']) {
  forbidMatch(read(path), /quay\.io\/minio|dl\.min\.io/u, `${path} must not reference the withdrawn MinIO distribution channels.`)
}

if (failures.length) {
  console.error(failures.map((failure) => `- ${failure}`).join('\n'))
  process.exit(1)
}

console.log('Object-storage source build, image references and media stack checks passed.')
