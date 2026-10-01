import { execFileSync } from 'node:child_process'
import { isolatedTestEnvironment } from '../testing/isolated-test-environment.mjs'

const variants = [
  { name: 'local', args: ['-f', 'compose.yaml'] },
  { name: 'debug', args: ['-f', 'compose.yaml', '-f', 'infra/compose/debug.yaml', '--profile', 'object-storage'] },
  { name: 'local-production', args: ['-f', 'compose.yaml', '-f', 'infra/compose/local-production.yaml'] },
  { name: 'e2e', args: [], environment: isolatedTestEnvironment() },
  { name: 'production-app', args: ['--env-file', 'infra/deploy/production.env.example', '-f', 'infra/compose/production.app.yaml'] },
  { name: 'production-infrastructure', args: ['--env-file', 'infra/deploy/infrastructure.env.example', '-f', 'infra/compose/production.infrastructure.yaml', '--profile', 'bootstrap'] },
]
for (const variant of variants) {
  execFileSync('docker', ['compose', '--project-directory', '.', ...variant.args, 'config', '--quiet'], {
    env: { ...process.env, ...variant.environment }, stdio: 'inherit',
  })
  console.log(`Compose configuration passed: ${variant.name}`)
}

// Platform selection must remain optional for existing host-native deployments.
for (const platform of [undefined, '', 'linux/arm64', 'linux/amd64']) {
  for (const variant of variants.filter(entry => entry.name.startsWith('production-'))) {
    const environment = { ...process.env, COMPOSE_DISABLE_ENV_FILE: '1' }
    delete environment.AIRTEK_TARGET_PLATFORM
    if (platform !== undefined) environment.AIRTEK_TARGET_PLATFORM = platform
    const config = JSON.parse(execFileSync('docker', ['compose', '--project-directory', '.', ...variant.args, 'config', '--format', 'json'], { env: environment, encoding: 'utf8' }))
    for (const [name, service] of Object.entries(config.services)) {
      if ((service.platform ?? '') !== (platform ?? '')) throw new Error(`${variant.name}/${name}: incorrect optional platform`)
    }
  }
}
console.log('Optional platform selection passed: unset, empty, ARM64, AMD64.')
