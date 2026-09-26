import { existsSync, readdirSync, readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
const root = dirname(dirname(dirname(fileURLToPath(import.meta.url))))
const failures = []
function read(relativePath) {
  const path = join(root, relativePath)
  if (!existsSync(path)) {
    failures.push(`${relativePath} is missing.`)
    return ''
  }
  return readFileSync(path, 'utf8').replace(/\r\n/gu, '\n')
}

function requireMatch(body, pattern, failure) {
  if (!pattern.test(body)) failures.push(failure)
}

function forbidMatch(body, pattern, failure) {
  if (pattern.test(body)) failures.push(failure)
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, '\\$&')
}

function serviceBlock(body, name) {
  const marker = `  ${name}:\n`
  const start = body.indexOf(marker)
  if (start < 0) {
    failures.push(`Compose service ${name} is missing.`)
    return ''
  }
  const remainder = body.slice(start + marker.length)
  const nextService = remainder.search(/^  [a-z0-9][a-z0-9-]*:\s*$/mu)
  return nextService < 0 ? remainder : remainder.slice(0, nextService)
}

const dockerignore = read('.dockerignore')
for (const marker of ['.git', '**/node_modules', '**/dist', '**/target', 'docs']) {
  requireMatch(dockerignore, new RegExp(`^${marker.replace(/[.*+?^${}()|[\]\\]/gu, '\\$&')}$`, 'mu'), `.dockerignore must exclude ${marker}.`)
}

const webDockerfile = read('infra/docker/Dockerfile.web')
requireMatch(webDockerfile, /^FROM\s+node:22-alpine\s+AS\s+runtime$/mu, 'Public Web Dockerfile needs a separate runtime stage.')
requireMatch(webDockerfile, /^USER\s+node$/mu, 'Public Web runtime must use the non-root node user.')
requireMatch(webDockerfile, /deploy\s+--prod\s+--legacy\s+\/runtime/u, 'Public Web runtime dependencies must be production-only.')
requireMatch(webDockerfile, /ARG\s+VITE_PUBLIC_API_BASE_URL/u, 'Public Web Dockerfile must accept the browser API build argument.')
for (const variable of ['NPM_CONFIG_REGISTRY', 'HTTP_PROXY', 'HTTPS_PROXY', 'NO_PROXY']) {
  requireMatch(webDockerfile, new RegExp(`ARG\\s+${variable}`, 'u'), `Public Web Dockerfile must accept ${variable} during builds.`)
}
requireMatch(webDockerfile, /COREPACK_NPM_REGISTRY=\$\{NPM_CONFIG_REGISTRY\}/u, 'Public Web must route Corepack through the selected npm registry.')
requireMatch(webDockerfile, /NODE_USE_ENV_PROXY=1/u, 'Public Web Node build must honor explicit proxy arguments.')
requireMatch(webDockerfile, /NPM_CONFIG_REGISTRY=\$\{NPM_CONFIG_REGISTRY\}/u, 'Public Web dependencies must use the selected npm registry.')
requireMatch(webDockerfile, /pnpm config set registry "\$\{NPM_CONFIG_REGISTRY\}" --location=project/u, 'Public Web must persist the selected registry for every pnpm build command.')
requireMatch(webDockerfile, /^EXPOSE\s+3000$/mu, 'Public Web image must expose only its fixed application port 3000.')
forbidMatch(webDockerfile, /ARG\s+PUBLIC_ENV__/u, 'Public Web Dockerfile still uses a non-exposed Vite environment prefix.')

const webVite = read('apps/web/vite.config.ts')
requireMatch(webVite, /port:\s*3000/u, 'Public Vite development and preview servers must use port 3000.')
requireMatch(webVite, /strictPort:\s*true/u, 'Public Vite must fail instead of selecting a different port.')
const platformDockerfile = read('infra/docker/Dockerfile.platform')
requireMatch(platformDockerfile, /ARG\s+AIRTEK_PLATFORM_FEATURES=production/u, 'Platform image feature selection must default to production only.')
requireMatch(platformDockerfile, /--features\s+"\$AIRTEK_PLATFORM_FEATURES"/u, 'Platform image must compile the explicitly selected feature set.')
for (const variable of ['CARGO_REGISTRY_MIRROR', 'HTTP_PROXY', 'HTTPS_PROXY', 'NO_PROXY']) {
  requireMatch(platformDockerfile, new RegExp(`ARG\\s+${variable}`, 'u'), `Platform Dockerfile must accept ${variable} during builds.`)
}
requireMatch(platformDockerfile, /replace-with = "airtek-mirror"/u, 'Platform must configure a named Cargo mirror only when requested.')
requireMatch(platformDockerfile, /^USER\s+10001$/mu, 'Platform runtime must remain non-root.')
requireMatch(platformDockerfile, /^EXPOSE\s+8080$/mu, 'Platform image must expose only its fixed API port 8080.')
forbidMatch(platformDockerfile, /airtekctl/u, 'Production Platform image must not contain the retired airtekctl.')
forbidMatch(platformDockerfile, /airtek-migrate/u, 'Production Platform image must not contain the retired SQLx migrator.')

const flywayDockerfile = read('infra/docker/Dockerfile.flyway')
requireMatch(flywayDockerfile, /^ARG\s+FLYWAY_BASE_IMAGE=flyway\/flyway:13\.4\.0-alpine$/mu, 'Flyway must use the reviewed, fixed OSS image version.')
requireMatch(flywayDockerfile, /COPY\s+services\/platform\/migrations\s+\/flyway\/project\/migrations/u, 'Flyway image must package the versioned SQL migrations.')
requireMatch(flywayDockerfile, /COPY\s+services\/platform\/flyway\/callbacks\s+\/flyway\/project\/flyway\/callbacks/u, 'Flyway image must package the guarded SQLx adoption callback.')
requireMatch(flywayDockerfile, /COPY\s+--chmod=0555\s+infra\/docker\/flyway-entrypoint\.sh/u, 'Flyway image must package the validated entrypoint.')
requireMatch(flywayDockerfile, /^USER\s+10001$/mu, 'Flyway runtime must use a non-root UID.')
requireMatch(flywayDockerfile, /^ENTRYPOINT\s+\["\/usr\/local\/bin\/airtek-flyway"\]$/mu, 'Flyway image must use the runtime-role validating entrypoint.')

const flywayEntrypoint = read('infra/docker/flyway-entrypoint.sh')
requireMatch(flywayEntrypoint, /FLYWAY_PLACEHOLDERS_RUNTIME_ROLE/u, 'Flyway entrypoint must validate the runtime-role placeholder.')
requireMatch(flywayEntrypoint, /-configFiles=\/flyway\/project\/flyway\.toml/u, 'Flyway entrypoint must load the reviewed configuration explicitly.')
requireMatch(flywayEntrypoint, /"-placeholders\.runtime_role=\$runtime_role"/u, 'Flyway entrypoint must bind the validated runtime role explicitly.')
requireMatch(flywayEntrypoint, /The Flyway DDL role and application runtime role must be distinct/u, 'Flyway entrypoint must reject a shared production database role.')
requireMatch(flywayEntrypoint, /if \[ "\$#" -ne 1 \]/u, 'Flyway entrypoint must reject commands with extra configuration arguments.')
requireMatch(flywayEntrypoint, /baseline\|migrate\|info\|validate/u, 'Flyway entrypoint must allow only the reviewed command set.')
requireMatch(flywayEntrypoint, /Unsupported Flyway command/u, 'Flyway entrypoint must reject repair and every unreviewed command.')
for (const fixedArgument of [
  /-locations=filesystem:\/flyway\/project\/migrations/u,
  /-callbackLocations=filesystem:\/flyway\/project\/flyway\/callbacks/u,
  /-defaultSchema=public/u,
  /-schemas=public/u,
  /-table=flyway_schema_history/u,
  /-encoding=UTF-8/u,
  /-executeInTransaction=true/u,
  /-failOnMissingLocations=true/u,
  /-baselineOnMigrate=false/u,
  /-cleanDisabled=true/u,
  /-ignoreMigrationPatterns=\*:future/u,
  /-outOfOrder=false/u,
  /-skipDefaultCallbacks=false/u,
  /-skipDefaultResolvers=false/u,
  /-validateMigrationNaming=true/u,
  /-validateOnMigrate=true/u,
  /-placeholderReplacement=true/u,
  /-sqlMigrationPrefix=V/u,
  /-sqlMigrationSeparator=__/u,
  /-sqlMigrationSuffixes=\.sql/u,
]) {
  requireMatch(flywayEntrypoint, fixedArgument, `Flyway entrypoint must pin safety argument ${fixedArgument.source}.`)
}
forbidMatch(flywayEntrypoint, /"\$@"/u, 'Flyway entrypoint must not forward caller-supplied configuration arguments.')
requireMatch(flywayEntrypoint, /-baselineVersion=10/u, 'Flyway entrypoint must fix the guarded SQLx adoption baseline at version 10.')
requireMatch(flywayEntrypoint, /-connectRetries=10/u, 'Flyway entrypoint must retain bounded database connection retries.')
requireMatch(flywayEntrypoint, /-skipExecutingMigrations=false/u, 'Flyway entrypoint must always execute pending migrations.')
requireMatch(flywayEntrypoint, /exec\s+flyway/u, 'Flyway entrypoint must replace itself with the Flyway CLI.')

const beforeBaseline = read('services/platform/flyway/callbacks/beforeBaseline.sql')
requireMatch(beforeBaseline, /actual\.success IS DISTINCT FROM TRUE/u, 'Flyway baseline validation must reject null or failed SQLx migration rows.')
requireMatch(beforeBaseline, /actual\.checksum IS DISTINCT FROM decode/u, 'Flyway baseline validation must reject null or changed SQLx checksums.')
requireMatch(beforeBaseline, /relation\.relname = '_sqlx_migrations'[\s\S]*relation\.relkind IN \('r', 'p'\)/u, 'Flyway baseline validation must require the SQLx history object to be a table.')
requireMatch(beforeBaseline, /count\(\*\), count\(DISTINCT version\)/u, 'Flyway baseline validation must reject duplicate or non-exact SQLx history.')
for (const legacyTable of ['content_entries', 'jobs']) {
  requireMatch(beforeBaseline, new RegExp(`'${legacyTable}'`, 'u'), `Flyway baseline validation must require legacy table ${legacyTable}.`)
}
const beforeMigrate = read('services/platform/flyway/callbacks/beforeMigrate.sql')
requireMatch(beforeMigrate, /to_regclass\('public\.flyway_schema_history'\) IS NULL[\s\S]*RETURN/u, 'Fresh Flyway migrations must bypass the legacy-adoption evidence check.')
requireMatch(beforeMigrate, /type = 'BASELINE'[\s\S]*version = '10'[\s\S]*success IS TRUE/u, 'Flyway migrate must detect every successful version 10 baseline.')
requireMatch(beforeMigrate, /IF NOT has_legacy_baseline THEN[\s\S]*RETURN/u, 'Flyway SQL migration histories must bypass the legacy-adoption evidence check.')
requireMatch(beforeMigrate, /SQLx history exists without the reviewed version 10 baseline/u, 'Flyway migrate must reject an unbaselined legacy SQLx database.')
requireMatch(beforeMigrate, /relation\.relname = '_sqlx_migrations'[\s\S]*relation\.relkind IN \('r', 'p'\)/u, 'Flyway migrate must require SQLx history to remain a table after baseline adoption.')
requireMatch(beforeMigrate, /count\(\*\), count\(DISTINCT version\)/u, 'Flyway migrate must reject duplicate or non-exact SQLx adoption history.')
requireMatch(beforeMigrate, /actual\.success IS DISTINCT FROM TRUE/u, 'Flyway migrate must reject failed SQLx adoption evidence.')
requireMatch(beforeMigrate, /actual\.checksum IS DISTINCT FROM decode/u, 'Flyway migrate must reject changed SQLx adoption checksums.')
for (const legacyTable of ['content_entries', 'jobs']) {
  requireMatch(beforeMigrate, new RegExp(`'${legacyTable}'`, 'u'), `Flyway migrate must require adopted legacy table ${legacyTable}.`)
}
const baselineChecksums = beforeBaseline.match(/\b[0-9a-f]{96}\b/gu) ?? []
const migrateChecksums = beforeMigrate.match(/\b[0-9a-f]{96}\b/gu) ?? []
if (baselineChecksums.length !== 10 || baselineChecksums.join(',') !== migrateChecksums.join(',')) {
  failures.push('Flyway baseline and migrate callbacks must enforce the same ten immutable SQLx checksums.')
}
const afterMigrate = read('services/platform/flyway/callbacks/afterMigrate.sql')
requireMatch(afterMigrate, /GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public/u, 'Flyway must grant application DML after migration.')
requireMatch(afterMigrate, /REVOKE INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER ON TABLE public\.flyway_schema_history/u, 'Flyway history must remain read-only to the runtime role.')
requireMatch(afterMigrate, /REVOKE CREATE ON SCHEMA public FROM PUBLIC/u, 'Flyway must close the legacy PUBLIC schema-create grant.')
requireMatch(afterMigrate, /rolcanlogin/u, 'Flyway must reject a runtime role that cannot log in.')
requireMatch(afterMigrate, /GRANT CONNECT ON DATABASE/u, 'Flyway must grant the application runtime role database CONNECT.')
requireMatch(afterMigrate, /has_database_privilege\(runtime_role, current_database\(\), 'CONNECT'\)/u, 'Flyway must verify the application runtime role database CONNECT grant.')
requireMatch(afterMigrate, /GRANT TEMPORARY ON DATABASE/u, 'Flyway must preserve the Worker temporary-table capability.')
for (const roleAttribute of ['rolsuper', 'rolcreatedb', 'rolcreaterole', 'rolreplication', 'rolbypassrls']) {
  requireMatch(afterMigrate, new RegExp(roleAttribute, 'u'), `Flyway must reject runtime role attribute ${roleAttribute}.`)
}
requireMatch(afterMigrate, /pg_has_role\(runtime_role, target_role\.oid, 'SET'\)/u, 'Flyway must reject runtime roles that can SET ROLE to another identity.')
requireMatch(afterMigrate, /pg_has_role\(runtime_role, target_role\.oid, 'USAGE'\)/u, 'Flyway must reject runtime roles that inherit another role.')
requireMatch(afterMigrate, /has_table_privilege/u, 'Flyway must fail if runtime table grants are incomplete.')

const flywayConfig = read('services/platform/flyway.toml')
for (const required of [
  /validateMigrationNaming\s*=\s*true/u,
  /validateOnMigrate\s*=\s*true/u,
  /cleanDisabled\s*=\s*true/u,
  /outOfOrder\s*=\s*false/u,
  /baselineOnMigrate\s*=\s*false/u,
  /baselineVersion\s*=\s*"10"/u,
]) {
  requireMatch(flywayConfig, required, `Flyway safety setting ${required.source} is missing.`)
}
const migrationDirectory = join(root, 'services/platform/migrations')
const migrationFiles = existsSync(migrationDirectory)
  ? readdirSync(migrationDirectory).filter((file) => file.endsWith('.sql')).sort()
  : []
const migrationVersions = migrationFiles.map((file) => {
  const match = /^V(\d{4})__[a-z0-9_]+\.sql$/u.exec(file)
  if (!match) {
    failures.push(`Flyway migration ${file} does not use V####__description.sql naming.`)
    return 0
  }
  return Number(match[1])
})
const latestMigration = Math.max(0, ...migrationVersions)
const flywayTargetPattern = new RegExp(`^AIRTEK_FLYWAY_TARGET=${latestMigration}$`, 'mu')
const expectedMigrations = Array.from({ length: latestMigration }, (_, index) => index + 1)
const legacySqlxVersions = Array.from({ length: 10 }, (_, index) => index + 1)
if (latestMigration < 10 || !legacySqlxVersions.every((version) => migrationVersions.includes(version))) {
  failures.push('Flyway migration history V0001-V0010 must remain present.')
}
if (new Set(migrationVersions).size !== migrationVersions.length) {
  failures.push('Flyway migrations must have unique positive versions.')
}
if (migrationVersions.slice().sort((left, right) => left - right).join(',') !== expectedMigrations.join(',')) {
  failures.push('Flyway migration versions must be contiguous from V0001.')
}
if (migrationFiles.some((file) => /^\d{4}_/u.test(file))) failures.push('Legacy SQLx migration filenames must not remain in the Flyway location.')
const flywayBuild = read('services/platform/crates/runtime/build.rs')
requireMatch(flywayBuild, /Flyway migration versions must be contiguous from V1/u, 'Rust builds must derive their required schema version from the Flyway directory.')
requireMatch(flywayBuild, /LEGACY_SQLX_LAST_VERSION:\s*i64\s*=\s*10/u, 'Rust builds must lock the immutable SQLx v1-v10 migration history.')
requireMatch(flywayBuild, /Flyway migration history V1-V10 must remain present/u, 'Rust builds must reject deletion of an adopted SQLx migration.')
const flywayRuntime = read('services/platform/crates/runtime/src/flyway.rs')
requireMatch(flywayRuntime, /include!\(concat!\(env!\("OUT_DIR"\), "\/flyway_version\.rs"\)\)/u, 'Runtime readiness must use the build-derived Flyway version.')
requireMatch(flywayRuntime, /LEGACY_SQLX_BASELINE_VERSION:\s*i64\s*=\s*10/u, 'Runtime readiness must recognize only the reviewed SQLx v10 baseline.')
requireMatch(flywayRuntime, /migration_type = 'SQL'/u, 'Runtime readiness must count only successful Flyway SQL migrations as schema coverage.')

const adminDockerfile = read('infra/docker/Dockerfile.admin')
requireMatch(adminDockerfile, /\/etc\/nginx\/templates\/default\.conf\.template/u, 'Admin Nginx config must be rendered as an environment-aware template.')
requireMatch(adminDockerfile, /^ENV\s+VITE_ENABLE_DEVTOOLS=false$/mu, 'Admin production image must force DevTools off.')
for (const variable of ['NPM_CONFIG_REGISTRY', 'HTTP_PROXY', 'HTTPS_PROXY', 'NO_PROXY']) {
  requireMatch(adminDockerfile, new RegExp(`ARG\\s+${variable}`, 'u'), `Admin Dockerfile must accept ${variable} during builds.`)
}
requireMatch(adminDockerfile, /COREPACK_NPM_REGISTRY=\$\{NPM_CONFIG_REGISTRY\}/u, 'Admin must route Corepack through the selected npm registry.')
requireMatch(adminDockerfile, /pnpm config set registry "\$\{NPM_CONFIG_REGISTRY\}" --location=project/u, 'Admin must persist the selected registry for every pnpm build command.')
requireMatch(adminDockerfile, /NODE_USE_ENV_PROXY=1/u, 'Admin Node build must honor explicit proxy arguments.')
requireMatch(adminDockerfile, /^USER\s+nginx$/mu, 'Admin runtime must use the non-root nginx user.')
requireMatch(adminDockerfile, /^EXPOSE\s+3100$/mu, 'Admin image must expose only its fixed application port 3100.')

const adminVite = read('apps/admin/vite.config.ts')
requireMatch(adminVite, /port:\s*3100/u, 'Admin Vite development and preview servers must use port 3100.')
requireMatch(adminVite, /strictPort:\s*true/u, 'Admin Vite must fail instead of selecting a different port.')
requireMatch(adminVite, /apiConnectSources\(env\.VITE_ADMIN_API_BASE_URL\)/u, 'Admin development CSP must derive HTTP and WebSocket origins from VITE_ADMIN_API_BASE_URL.')
requireMatch(adminVite, /requestedDevtools\s*=\s*env\.VITE_ENABLE_DEVTOOLS\s*===\s*'true'/u, 'Admin DevTools must require an explicit development opt-in.')
forbidMatch(adminVite, /connect-src 'self' http:\/\/localhost:8080 ws:\/\/localhost:8080/u, 'Admin development CSP must not hard-code an API port that bypasses the root environment.')

const platformConfig = `${read('services/platform/crates/runtime/src/config.rs')}\n${read('services/platform/crates/runtime/src/config/types.rs')}`
requireMatch(platformConfig, /pub const API_PORT:\s*u16\s*=\s*8080;/u, 'Rust API must use the fixed application port 8080.')
const platformApi = read('services/platform/apps/api/src/main.rs')
requireMatch(platformApi, /AppState::new\(config\)\?/u, 'The running Rust API must construct the PostgreSQL-only application state.')
requireMatch(platformApi, /verify_runtime_ready\(\)\.await\?/u, 'The running Rust API must verify one-shot runtime preparation.')

const gatewayDockerfile = read('infra/docker/Dockerfile.gateway')
requireMatch(gatewayDockerfile, /infra\/gateway\/nginx\.conf\.template/u, 'Gateway image must package the checked-in Host router.')
requireMatch(gatewayDockerfile, /^USER\s+nginx$/mu, 'Gateway runtime must use the non-root nginx user.')
requireMatch(gatewayDockerfile, /^EXPOSE\s+8088$/mu, 'Gateway image must expose its unprivileged port 8088.')

const compose = read('compose.yaml')
const minioImage = 'quay.io/minio/minio:RELEASE.2025-09-07T16-13-09Z@sha256:14cea493d9a34af32f524e538b8346cf79f3321eff8e708c1e2960462bd8936e'
const minioMcImage = 'quay.io/minio/mc:RELEASE.2025-08-13T08-35-41Z@sha256:a7fe349ef4bd8521fb8497f55c6042871b2ae640607cf99d9bede5e9bdf11727'
requireMatch(compose, /^\s{2}flyway-migrate:\s*$/mu, 'Compose must define the one-shot Flyway migration service.')
requireMatch(compose, /^\s{2}platform-maintenance:\s*$/mu, 'Compose must define one-shot runtime data preparation.')
requireMatch(serviceBlock(compose, 'flyway-migrate'), /dockerfile:\s*infra\/docker\/Dockerfile\.flyway/u, 'Compose migration service must build the Flyway image.')
for (const variable of ['FLYWAY_URL', 'FLYWAY_USER', 'FLYWAY_PASSWORD', 'FLYWAY_PLACEHOLDERS_RUNTIME_ROLE']) {
  requireMatch(serviceBlock(compose, 'flyway-migrate'), new RegExp(`${variable}:\\s*\\$\\{${variable}`, 'u'), `Local Flyway must receive ${variable} from .env.`)
}
requireMatch(serviceBlock(compose, 'flyway-migrate'), new RegExp(`AIRTEK_FLYWAY_TARGET:\\s*\\$\\{AIRTEK_FLYWAY_TARGET:-${latestMigration}\\}`, 'u'), `Local Compose must target schema V${latestMigration}.`)
requireMatch(serviceBlock(compose, 'flyway-migrate'), /AIRTEK_FLYWAY_ALLOW_SHARED_ROLE:\s*"true"/u, 'Local Compose must make its shared development role exception explicit.')
forbidMatch(serviceBlock(compose, 'flyway-migrate'), /DATABASE_URL/u, 'Flyway must use its JDBC deployment credentials, not the Rust DATABASE_URL.')
requireMatch(serviceBlock(compose, 'platform-maintenance'), /entrypoint:\s*\["\/usr\/local\/bin\/airtek-maintenance"\]/u, 'Local maintenance must use the dedicated binary.')
requireMatch(serviceBlock(compose, 'platform-maintenance'), /command:\s*\["\$\{AIRTEK_MAINTENANCE_COMMAND:-prepare-development-runtime\}"\]/u, 'Local maintenance must prepare runtime data and the optional development administrator.')
for (const marker of [
  /AIRTEK_DEV_ADMIN_SEED:\s*\$\{AIRTEK_DEV_ADMIN_SEED:-true\}/u,
  /AIRTEK_DEV_ADMIN_DISPLAY_NAME:\s*AIRTEK Local Administrator/u,
  /AIRTEK_DEV_ADMIN_EMAIL:\s*local-admin@airtek\.invalid/u,
  /AIRTEK_DEV_ADMIN_PASSWORD:\s*Airtek-Local-Admin-20260917!/u,
]) {
  requireMatch(serviceBlock(compose, 'platform-maintenance'), marker, 'Local maintenance must own the fixed development administrator configuration.')
}
requireMatch(serviceBlock(compose, 'platform-maintenance'), /flyway-migrate:[\s\S]*condition:\s*service_completed_successfully/u, 'Local maintenance must wait for Flyway.')
for (const service of ['platform-api', 'platform-worker']) {
  requireMatch(serviceBlock(compose, service), /platform-maintenance:[\s\S]*condition:\s*service_completed_successfully/u, `${service} must wait for runtime preparation.`)
}
const migrationWaits = compose.match(/condition:\s*service_completed_successfully/gu)?.length ?? 0
if (migrationWaits < 2) failures.push('Both API and Worker must wait for a successful migration service.')
requireMatch(compose, /^\s{2}gateway:\s*$/mu, 'Compose must define the HTTP gateway service.')
requireMatch(compose, /dockerfile:\s*infra\/docker\/Dockerfile\.gateway/u, 'Compose must build the non-root gateway image.')
requireMatch(compose, /PUBLIC_API_INTERNAL_URL:\s*\$\{PUBLIC_API_INTERNAL_URL:-http:\/\/platform-api:8080\/api\/public\/v1\}/u, 'Public SSR must receive a configurable internal API URL with the fixed service default.')
requireMatch(compose, /AIRTEK_ADMIN_BOOTSTRAP_TOKEN:/u, 'Compose must configure the setup-only admin bootstrap secret.')
forbidMatch(compose, /AIRTEK_ADMIN_BEARER_TOKEN:/u, 'Compose must not present the setup secret as a reusable bearer token.')
for (const [variable, hostPort, containerPort] of [
  ['AIRTEK_API_HOST_PORT', 8080, 8080],
  ['AIRTEK_PUBLIC_HOST_PORT', 3000, 3000],
  ['AIRTEK_ADMIN_HOST_PORT', 3100, 3100],
  ['AIRTEK_GATEWAY_HOST_PORT', 8088, 8088],
]) {
  requireMatch(
    compose,
    new RegExp(`\\$\\{AIRTEK_BIND_ADDRESS:-127\\.0\\.0\\.1\\}:\\$\\{${variable}:-${hostPort}\\}:${containerPort}`, 'u'),
    `Local diagnostic port ${containerPort} must have a configurable loopback host mapping.`,
  )
}
for (const variable of ['POSTGRES_DB', 'POSTGRES_USER', 'POSTGRES_PASSWORD', 'AIRTEK_DATABASE_URL_INTERNAL']) {
  requireMatch(compose, new RegExp(`\\$\\{${variable}`, 'u'), `Base Compose must configure ${variable} through .env.`)
}
requireMatch(compose, /AIRTEK_TOTP_ENCRYPTION_KEY:\s*\$\{AIRTEK_TOTP_ENCRYPTION_KEY:-\}/u, 'Local API must receive the optional TOTP encryption key from .env.')
for (const variable of ['AIRTEK_PRODUCT_STAGING_ENCRYPTION_KEY', 'AIRTEK_ANALYTICS_TOKEN_HMAC_KEY', 'AIRTEK_INVITATION_REPLAY_ENCRYPTION_KEY', 'AIRTEK_GUEST_RAW_RETENTION_DAYS', 'AIRTEK_GUEST_AGGREGATE_RETENTION_MONTHS', 'AIRTEK_PRODUCT_IMPORT_MAPPING_VERSION', 'AIRTEK_APPROVED_PRODUCT_MASTER_SHA256', 'AIRTEK_APPROVED_PRODUCT_MASTER_MAPPING_VERSION', 'AIRTEK_APPROVED_PRODUCT_MASTER_VALID_ROWS', 'AIRTEK_APPROVED_PRODUCT_MASTER_ERROR_ROWS', 'AIRTEK_ANALYTICS_ALLOWED_UTM_SOURCES', 'AIRTEK_ANALYTICS_ALLOWED_UTM_MEDIUMS', 'AIRTEK_ANALYTICS_ALLOWED_UTM_CAMPAIGNS']) {
  requireMatch(serviceBlock(compose, 'platform-api'), new RegExp(`${variable}:\\s*\\$\\{${variable}`, 'u'), `Local API must receive ${variable} from .env.`)
  requireMatch(serviceBlock(compose, 'platform-worker'), new RegExp(`${variable}:\\s*\\$\\{${variable}`, 'u'), `Local Worker must receive ${variable} from .env.`)
}
forbidMatch(serviceBlock(compose, 'postgres'), /^\s+ports:/mu, 'Base Compose must not publish PostgreSQL to the host.')
requireMatch(serviceBlock(compose, 'minio'), /profiles:\s*\["minio"\]/u, 'Local MinIO must be controlled by the minio Compose profile.')
requireMatch(serviceBlock(compose, 'minio-create-bucket'), /profiles:\s*\["minio"\]/u, 'Local bucket initialization must use the minio Compose profile.')
requireMatch(serviceBlock(compose, 'minio'), /127\.0\.0\.1:\$\{AIRTEK_OBJECT_STORE_HOST_PORT:-19000\}:9000[\s\S]*127\.0\.0\.1:\$\{AIRTEK_OBJECT_STORE_CONSOLE_HOST_PORT:-19001\}:9001/u, 'Profiled MinIO ports must be configurable and loopback-only.')
requireMatch(serviceBlock(compose, 'minio-create-bucket'), /mc mb --ignore-existing/u, 'Local object storage must create its media bucket idempotently.')
requireMatch(serviceBlock(compose, 'minio'), new RegExp(`image:\\s*${escapeRegExp(minioImage)}`, 'u'), 'Local MinIO must use the reviewed multi-architecture manifest digest.')
requireMatch(serviceBlock(compose, 'minio'), /healthcheck:[\s\S]*\/minio\/health\/live/u, 'Local MinIO must expose a container healthcheck.')
requireMatch(serviceBlock(compose, 'minio-create-bucket'), new RegExp(`image:\\s*${escapeRegExp(minioMcImage)}`, 'u'), 'Local mc must use the reviewed multi-architecture manifest digest.')
requireMatch(serviceBlock(compose, 'minio-create-bucket'), /mc anonymous set download/u, 'Local media objects must be anonymously readable through their immutable URLs.')
requireMatch(serviceBlock(compose, 'minio-create-bucket'), /depends_on:[\s\S]*minio:[\s\S]*condition:\s*service_healthy/u, 'Local bucket initialization must wait for a healthy MinIO server.')
forbidMatch(serviceBlock(compose, 'platform-worker'), /^\s+(?:ports|expose):/mu, 'Worker must not expose or publish a listening port.')
forbidMatch(serviceBlock(compose, 'public-web'), /DATABASE_URL/u, 'Public SSR must not receive database credentials.')
forbidMatch(serviceBlock(compose, 'admin-web'), /DATABASE_URL/u, 'Admin SPA must not receive database credentials.')
requireMatch(compose, /ipv4_address:\s*\$\{AIRTEK_GATEWAY_INTERNAL_IP:-172\.28\.0\.10\}/u, 'Local gateway must have the fixed address used by the trusted-proxy policy.')
requireMatch(compose, /AIRTEK_TRUSTED_PROXY_CIDRS:\s*\$\{AIRTEK_TRUSTED_PROXY_CIDRS:-172\.28\.0\.10\/32\}/u, 'API must trust only the fixed local gateway by default.')
requireMatch(serviceBlock(compose, 'public-web'), /dockerfile:\s*infra\/docker\/Dockerfile\.web/u, 'Public must have its own image build.')
requireMatch(serviceBlock(compose, 'admin-web'), /dockerfile:\s*infra\/docker\/Dockerfile\.admin/u, 'Admin must have its own image build.')
requireMatch(serviceBlock(compose, 'platform-api'), /dockerfile:\s*infra\/docker\/Dockerfile\.platform/u, 'API must have its own process image boundary.')
for (const service of ['public-web', 'admin-web']) {
  const block = serviceBlock(compose, service)
  requireMatch(block, /NPM_CONFIG_REGISTRY:\s*\$\{AIRTEK_NPM_REGISTRY:-https:\/\/registry\.npmjs\.org\}/u, `${service} must default to the official npm registry.`)
  requireMatch(block, /HTTP_PROXY:\s*\$\{AIRTEK_BUILD_PROXY:-\}/u, `${service} must expose the optional build proxy.`)
}
for (const service of ['platform-maintenance', 'platform-api', 'platform-worker']) {
  const block = serviceBlock(compose, service)
  requireMatch(block, /AIRTEK_PLATFORM_FEATURES:\s*\$\{AIRTEK_PLATFORM_FEATURES:-devtools\}/u, `${service} must default to the local devtools feature set.`)
  requireMatch(block, /CARGO_REGISTRY_MIRROR:\s*\$\{AIRTEK_CARGO_MIRROR:-\}/u, `${service} must expose the optional Cargo mirror.`)
  requireMatch(block, /HTTPS_PROXY:\s*\$\{AIRTEK_BUILD_PROXY:-\}/u, `${service} must expose the optional build proxy.`)
}
forbidMatch(serviceBlock(compose, 'platform-api'), /AIRTEK_MEDIA_|S3_/u, 'The API must load S3 settings from PostgreSQL, not environment variables.')
forbidMatch(serviceBlock(compose, 'platform-worker'), /AIRTEK_MEDIA_|S3_/u, 'The ordinary Worker must not receive media storage configuration.')
for (const service of ['platform-api', 'platform-worker', 'admin-web', 'public-web']) {
  forbidMatch(serviceBlock(compose, service), /AIRTEK_DEV_ADMIN_(?:SEED|DISPLAY_NAME|EMAIL|PASSWORD):|local-admin@airtek\.invalid|Airtek-Local-Admin-20260917!/u, `${service} must not receive fixed development administrator credentials.`)
}
const debugCompose = read('infra/compose/debug.yaml')
for (const [variable, hostPort, containerPort] of [['AIRTEK_POSTGRES_DEBUG_PORT', 54320, 5432]]) {
  const mapping = `\\$\\{AIRTEK_BIND_ADDRESS:-127\\.0\\.0\\.1\\}:\\$\\{${variable}:-${hostPort}\\}:${containerPort}`
  requireMatch(debugCompose, new RegExp(mapping, 'u'), `Opt-in ${containerPort} diagnostic mapping must be configurable and loopback-only.`)
}

const productionCompose = read('infra/compose/production.app.yaml')
const productionEnvironment = read('infra/deploy/production.env.example')
for (const [body, label] of [[productionCompose, 'Production Compose'], [productionEnvironment, 'Production environment example'], [platformDockerfile, 'Platform Dockerfile']]) {
  forbidMatch(body, /AIRTEK_DEV_(?:ADMIN|PUBLIC)_|local-admin@airtek\.invalid|Airtek-Local-Admin-20260917!|prepare-development-runtime|reset-development-admin/u, `${label} must not contain development fixture provisioning or credentials.`)
}
requireMatch(serviceBlock(productionCompose, 'flyway-migrate'), new RegExp(`AIRTEK_FLYWAY_TARGET:\\s*\\$\\{AIRTEK_FLYWAY_TARGET:\\?set AIRTEK_FLYWAY_TARGET=${latestMigration}\\}`, 'u'), `Production Compose must target schema V${latestMigration}.`)
forbidMatch(productionCompose, /e2e-test/u, 'Production Compose must never reference the isolated E2E feature.')
forbidMatch(productionCompose, /^\s+build:\s*$/mu, 'Production Compose must promote immutable images instead of building from a checkout.')
for (const imageVariable of ['AIRTEK_PUBLIC_WEB_IMAGE', 'AIRTEK_ADMIN_WEB_IMAGE', 'AIRTEK_PLATFORM_IMAGE', 'AIRTEK_MIGRATIONS_IMAGE', 'AIRTEK_GATEWAY_IMAGE']) {
  requireMatch(
    productionCompose,
    new RegExp(`image:\\s*\\$\\{${imageVariable}:\\?`, 'u'),
    `Production Compose must require ${imageVariable}.`,
  )
}
forbidMatch(productionCompose, /image:\s*[^\n]+:latest\s*$/mu, 'Production Compose must not use a mutable latest image tag.')
for (const [service, port] of [['public-web', 3000], ['admin-web', 3100], ['platform-api', 8080]]) {
  const block = serviceBlock(productionCompose, service)
  requireMatch(block, new RegExp(`expose:[\\s\\S]*- "${port}"`, 'u'), `Production ${service} must expose fixed internal port ${port}.`)
  forbidMatch(block, /^\s+ports:/mu, `Production ${service} must not publish its internal port.`)
}
forbidMatch(serviceBlock(productionCompose, 'platform-worker'), /^\s+(?:ports|expose):/mu, 'Production Worker must not expose or publish a listening port.')
for (const service of ['flyway-migrate', 'platform-maintenance', 'platform-api', 'platform-worker', 'public-web', 'admin-web', 'public-readiness']) {
  forbidMatch(serviceBlock(productionCompose, service), /^\s+ports:/mu, `Production ${service} must not publish an application port.`)
}
forbidMatch(serviceBlock(productionCompose, 'public-web'), /DATABASE_URL/u, 'Production Public SSR must not receive database credentials.')
forbidMatch(serviceBlock(productionCompose, 'admin-web'), /DATABASE_URL/u, 'Production Admin SPA must not receive database credentials.')
forbidMatch(serviceBlock(productionCompose, 'platform-api'), /AIRTEK_MEDIA_|S3_/u, 'Production API must load S3 settings from PostgreSQL.')
forbidMatch(serviceBlock(productionCompose, 'platform-worker'), /AIRTEK_MEDIA_|S3_/u, 'Production Worker must not receive media storage configuration.')
requireMatch(serviceBlock(productionCompose, 'platform-worker'), /mem_limit:\s*768m/u, 'Production Worker must retain its memory boundary.')
requireMatch(productionCompose, /AIRTEK_INGRESS_BIND_ADDRESS:-127\.0\.0\.1/u, 'Production gateway must bind its outer-ingress listener to loopback by default.')
requireMatch(serviceBlock(productionCompose, 'platform-maintenance'), /entrypoint:\s*\["\/usr\/local\/bin\/airtek-maintenance"\]/u, 'Production maintenance must use the dedicated binary.')
requireMatch(serviceBlock(productionCompose, 'platform-maintenance'), /command:\s*\["prepare-runtime"\]/u, 'Production maintenance must prepare runtime data.')
requireMatch(serviceBlock(productionCompose, 'platform-maintenance'), /flyway-migrate:[\s\S]*condition:\s*service_completed_successfully/u, 'Production maintenance must wait for Flyway.')
for (const service of ['platform-api', 'platform-worker']) {
  requireMatch(serviceBlock(productionCompose, service), /platform-maintenance:[\s\S]*condition:\s*service_completed_successfully/u, `Production ${service} must wait for runtime preparation.`)
}

const productionEnv = read('infra/deploy/production.env.example')
forbidMatch(productionEnv, /e2e-test/u, 'Production environment configuration must never enable the isolated E2E feature.')
for (const origin of ['AIRTEK_PUBLIC_ORIGIN=https://', 'AIRTEK_ADMIN_ORIGIN=https://', 'AIRTEK_API_ORIGIN=https://']) {
  requireMatch(productionEnv, new RegExp(`^${origin}`, 'mu'), `Production environment example must configure ${origin.split('=')[0]} as HTTPS.`)
}
requireMatch(productionEnv, flywayTargetPattern, `Production must target schema V${latestMigration}.`)
forbidMatch(productionEnv, /^(?:AIRTEK_MEDIA_|AIRTEK_MINIO_|MINIO_|S3_)/mu, 'Production environment examples must not contain S3 or MinIO settings.')
requireMatch(productionCompose, /AIRTEK_TOTP_ENCRYPTION_KEY:\s*\$\{AIRTEK_TOTP_ENCRYPTION_KEY:\?/u, 'Production must require a secret-manager TOTP encryption key.')
requireMatch(productionEnv, /^AIRTEK_TOTP_ENCRYPTION_KEY=REPLACE_/mu, 'Production environment example must declare the TOTP key placeholder.')
for (const variable of ['FLYWAY_URL', 'FLYWAY_USER', 'FLYWAY_PASSWORD', 'FLYWAY_PLACEHOLDERS_RUNTIME_ROLE']) {
  requireMatch(serviceBlock(productionCompose, 'flyway-migrate'), new RegExp(`${variable}:\\s*\\$\\{${variable}:\\?`, 'u'), `Production Flyway must require ${variable}.`)
  requireMatch(productionEnv, new RegExp(`^${variable}=`, 'mu'), `Production environment example must declare ${variable}.`)
}
forbidMatch(serviceBlock(productionCompose, 'flyway-migrate'), /DATABASE_URL/u, 'Production Flyway must use separate JDBC deployment credentials.')
forbidMatch(serviceBlock(productionCompose, 'flyway-migrate'), /AIRTEK_FLYWAY_ALLOW_SHARED_ROLE/u, 'Production must never allow a shared Flyway/runtime role.')
for (const variable of ['AIRTEK_PRODUCT_STAGING_ENCRYPTION_KEY', 'AIRTEK_ANALYTICS_TOKEN_HMAC_KEY', 'AIRTEK_INVITATION_REPLAY_ENCRYPTION_KEY']) {
  requireMatch(productionCompose, new RegExp(`${variable}:\\s*\\$\\{${variable}:\\?`, 'u'), `Production must require ${variable} from the secret manager.`)
  requireMatch(productionEnv, new RegExp(`^${variable}=REPLACE_`, 'mu'), `Production environment example must declare ${variable}.`)
}
for (const variable of ['AIRTEK_PRODUCT_IMPORT_MAPPING_VERSION', 'AIRTEK_APPROVED_PRODUCT_MASTER_SHA256', 'AIRTEK_APPROVED_PRODUCT_MASTER_MAPPING_VERSION', 'AIRTEK_APPROVED_PRODUCT_MASTER_VALID_ROWS', 'AIRTEK_APPROVED_PRODUCT_MASTER_ERROR_ROWS']) {
  for (const service of ['platform-api', 'platform-worker']) {
    requireMatch(serviceBlock(productionCompose, service), new RegExp(`${variable}:\\s*\\$\\{${variable}:\\?`, 'u'), `Production ${service} must require ${variable}.`)
  }
  requireMatch(productionEnv, new RegExp(`^${variable}=`, 'mu'), `Production environment example must declare ${variable}.`)
}
for (const variable of ['AIRTEK_ANALYTICS_ALLOWED_UTM_SOURCES', 'AIRTEK_ANALYTICS_ALLOWED_UTM_MEDIUMS', 'AIRTEK_ANALYTICS_ALLOWED_UTM_CAMPAIGNS']) {
  requireMatch(serviceBlock(productionCompose, 'platform-api'), new RegExp(`${variable}:\\s*\\$\\{${variable}`, 'u'), `Production API must receive ${variable}.`)
  requireMatch(productionEnv, new RegExp(`^${variable}=`, 'mu'), `Production environment example must declare ${variable}.`)
}

const productionRelease = read('.github/workflows/release-production.yml')
requireMatch(productionRelease, /^\s+packages:\s+write$/mu, 'Production image publishing must grant the workflow package write permission.')
requireMatch(productionRelease, /registry:\s+ghcr\.io/u, 'Production images must publish to GHCR.')
requireMatch(productionRelease, /password:\s*\$\{\{ github\.token \}\}/u, 'GHCR publishing must use the short-lived GitHub workflow token.')
requireMatch(productionRelease, /tags:\s+\$\{\{ needs\.prepare\.outputs\.image_prefix \}\}-\$\{\{ matrix\.image \}\}:\$\{\{ needs\.prepare\.outputs\.image_tag \}\}/u, 'Published image tags must use the AIRTEKPOWER package prefix and prepared immutable tag.')
forbidMatch(productionRelease, /ACR_/u, 'Production release workflow must not retain Alibaba Container Registry configuration.')
requireMatch(productionRelease, /image_tag=\$REQUESTED_SHA-candidate/u, 'Incomplete production origins must publish a distinct candidate tag.')
requireMatch(productionRelease, /public_origin=http:\/\/www\.localhost:8088/u, 'Candidate Public builds must use the reviewed local origin.')
requireMatch(productionRelease, /api_origin=http:\/\/api\.localhost:8088/u, 'Candidate browser builds must use the reviewed local API origin.')
forbidMatch(productionRelease, /^  deploy:\s*$/mu, 'Image publishing must not contain a server deployment job.')
forbidMatch(productionRelease, /ECS_|SSH_PRIVATE_KEY|deploy_after_publish/u, 'Image publishing must not accept server credentials or deployment controls.')

const productionDeploy = read('infra/deploy/deploy-app.sh')
requireMatch(productionDeploy, /compose_release "\$release_dir" run --rm flyway-migrate validate\ncompose_release "\$release_dir" run --rm platform-maintenance\n/u, 'Production deployment must prepare runtime data immediately after Flyway validation.')
requireMatch(productionDeploy, /AIRTEK_PLATFORM_IMAGE=\$image_prefix-platform:\$release_id/u, 'Production deployment must reference the GHCR AIRTEKPOWER platform package.')

const localEnvExample = read('.env.example')
for (const variable of [
  'AIRTEK_PUBLIC_HOST_PORT',
  'AIRTEK_ADMIN_HOST_PORT',
  'AIRTEK_API_HOST_PORT',
  'AIRTEK_GATEWAY_HOST_PORT',
  'COMPOSE_PROFILES',
  'AIRTEK_POSTGRES_DEBUG_PORT',
  'POSTGRES_DB',
  'POSTGRES_USER',
  'POSTGRES_PASSWORD',
  'AIRTEK_DATABASE_URL_INTERNAL',
  'DATABASE_URL',
  'AIRTEK_FLYWAY_BASE_IMAGE',
  'AIRTEK_NPM_REGISTRY',
  'AIRTEK_CARGO_MIRROR',
  'AIRTEK_BUILD_PROXY',
  'FLYWAY_URL',
  'FLYWAY_USER',
  'FLYWAY_PASSWORD',
  'FLYWAY_PLACEHOLDERS_RUNTIME_ROLE',
  'AIRTEK_TOTP_ENCRYPTION_KEY',
  'AIRTEK_PRODUCT_STAGING_ENCRYPTION_KEY',
  'AIRTEK_ANALYTICS_TOKEN_HMAC_KEY',
  'AIRTEK_INVITATION_REPLAY_ENCRYPTION_KEY',
  'AIRTEK_GUEST_RAW_RETENTION_DAYS',
  'AIRTEK_GUEST_AGGREGATE_RETENTION_MONTHS',
  'AIRTEK_PRODUCT_IMPORT_MAPPING_VERSION',
  'AIRTEK_APPROVED_PRODUCT_MASTER_SHA256',
  'AIRTEK_APPROVED_PRODUCT_MASTER_MAPPING_VERSION',
  'AIRTEK_APPROVED_PRODUCT_MASTER_VALID_ROWS',
  'AIRTEK_APPROVED_PRODUCT_MASTER_ERROR_ROWS',
  'AIRTEK_ANALYTICS_ALLOWED_UTM_SOURCES',
  'AIRTEK_ANALYTICS_ALLOWED_UTM_MEDIUMS',
  'AIRTEK_ANALYTICS_ALLOWED_UTM_CAMPAIGNS',
  'AIRTEK_TRUSTED_PROXY_CIDRS',
  'RUST_LOG',
]) {
  requireMatch(localEnvExample, new RegExp(`^${variable}=`, 'mu'), `.env.example must document ${variable}.`)
}
requireMatch(localEnvExample, flywayTargetPattern, `.env.example must target schema V${latestMigration}.`)
forbidMatch(localEnvExample, /^(?:AIRTEK_MEDIA_|AIRTEK_MINIO_|MINIO_|S3_)/mu, '.env.example must not contain S3 or MinIO settings.')
if (existsSync(join(root, '.env'))) {
  forbidMatch(read('.env'), /^(?:AIRTEK_MEDIA_|AIRTEK_MINIO_|MINIO_|S3_)/mu, 'Local .env must not contain S3 or MinIO settings.')
}

const adminNginx = read('apps/admin/deploy/nginx.conf')
for (const header of [
  'X-Robots-Tag',
  'X-Content-Type-Options',
  'X-Frame-Options',
  'Referrer-Policy',
  'Permissions-Policy',
  'Content-Security-Policy',
]) {
  requireMatch(adminNginx, new RegExp(`add_header\\s+${escapeRegExp(header)}\\s`, 'u'), `Admin Nginx is missing ${header}.`)
}
requireMatch(adminNginx, /site\\\.webmanifest/u, 'Admin Nginx must return 404 for public manifests.')
requireMatch(adminNginx, /google\[\^\/\]\+\\\.html/u, 'Admin Nginx must return 404 for webmaster verification files.')
requireMatch(adminNginx, /location\s+~\*\s+\^\/sitemap/u, 'Admin Nginx must reject every sitemap variant before SPA fallback.')
requireMatch(adminNginx, /location\s+~\*\s+\^\/en/u, 'Admin Nginx must reject public locale routes before SPA fallback.')
if ((adminNginx.match(/add_header\s+X-Robots-Tag/gu)?.length ?? 0) !== 1) {
  failures.push('Admin security headers must be declared once at server scope to avoid Nginx inheritance loss.')
}

const gateway = read('infra/gateway/nginx.conf.template')
requireMatch(gateway, /listen\s+8088\s+default_server/u, 'Gateway must reject unknown Host values through an unprivileged default server.')
requireMatch(gateway, /location\s+\^~\s+\/api\/devtools\/\s*\{\s*return\s+404;/u, 'Gateway must reject the production DevTools namespace.')
requireMatch(gateway, /location\s+\^~\s+\/internal\/\s*\{\s*return\s+404;/u, 'Gateway must reject internal metrics and diagnostics.')
requireMatch(gateway, /location\s+\^~\s+\/admin\/\s*\{\s*return\s+404;/u, 'Public gateway must reject the Admin namespace.')
requireMatch(gateway, /location\s+\^~\s+\/en\/\s*\{\s*return\s+404;/u, 'Admin gateway must reject public locale routes.')
requireMatch(gateway, /site\\\.webmanifest/u, 'Gateway must block admin public manifests.')
requireMatch(gateway, /return\s+200\s+"User-agent:\s*\*\\nDisallow:\s*\/\\n"/u, 'API gateway robots.txt must disallow all crawling.')
requireMatch(gateway, /Content-Security-Policy/u, 'Gateway must set per-origin CSP headers.')
if ((gateway.match(/add_header\s+Content-Security-Policy/gu)?.length ?? 0) !== 4) {
  failures.push('Every gateway server block, including the unknown-Host boundary, must define a CSP.')
}
if ((gateway.match(/proxy_hide_header\s+Content-Security-Policy/gu)?.length ?? 0) < 2) {
  failures.push('Gateway must replace upstream Public/Admin CSP values instead of intersecting duplicate policies.')
}
forbidMatch(gateway, /proxy_set_header\s+(?:Upgrade|Connection)/iu, 'Production gateway must not configure a WebSocket upstream.')

if (failures.length) {
  console.error(failures.map((failure) => `- ${failure}`).join('\n'))
  process.exit(1)
}

console.log('Deployment configuration checks passed.')
