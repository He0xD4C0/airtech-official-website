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
