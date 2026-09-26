#!/usr/bin/env node

import {
  existsSync,
  lstatSync,
  readFileSync,
  realpathSync,
  rmSync,
} from 'node:fs'
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'

export const CLEAN_TARGETS = [
  'apps/admin/dist',
  'apps/web/dist',
  'packages/contracts/dist',
  '.local/qa',
]

const PLATFORM_MANIFEST = 'services/platform/Cargo.toml'
const PLATFORM_TARGET = 'services/platform/target'
const EXPECTED_PACKAGE_NAME = 'airtekpower-platform'

function isWithin(root, candidate) {
  const pathFromRoot = relative(root, candidate)
  return pathFromRoot === '' || (!pathFromRoot.startsWith(`..${sep}`) && pathFromRoot !== '..')
}

function validateRepositoryRoot(root, cwd) {
  const absoluteRoot = resolve(root)
  if (resolve(cwd) !== absoluteRoot) {
    throw new Error(`Run this command from the repository root: ${absoluteRoot}`)
  }

  const packagePath = resolve(absoluteRoot, 'package.json')
  const manifestPath = resolve(absoluteRoot, PLATFORM_MANIFEST)
  if (!existsSync(packagePath) || !existsSync(manifestPath)) {
    throw new Error('Repository markers are missing; refusing to clean.')
  }

  const packageJson = JSON.parse(readFileSync(packagePath, 'utf8'))
  if (packageJson.name !== EXPECTED_PACKAGE_NAME) {
    throw new Error(`Unexpected package name ${JSON.stringify(packageJson.name)}; refusing to clean.`)
  }
  return absoluteRoot
}

function validateTarget(root, relativePath) {
  if (isAbsolute(relativePath) || relativePath.split(/[\\/]/u).includes('..')) {
    throw new Error(`Unsafe cleanup target: ${relativePath}`)
  }

  const absolutePath = resolve(root, relativePath)
  let ancestor = dirname(absolutePath)
  while (ancestor !== root && isWithin(root, ancestor)) {
    if (existsSync(ancestor) && lstatSync(ancestor).isSymbolicLink()) {
      throw new Error(`Cleanup target ancestor is a symbolic link: ${relativePath}`)
    }
    ancestor = dirname(ancestor)
  }
  if (!isWithin(root, absolutePath)) {
    throw new Error(`Cleanup target escapes the repository: ${relativePath}`)
  }
  if (!existsSync(absolutePath)) return { absolutePath, exists: false }

  const stats = lstatSync(absolutePath)
  if (stats.isSymbolicLink()) {
    throw new Error(`Cleanup target is a symbolic link: ${relativePath}`)
  }
  const realPath = realpathSync(absolutePath)
  if (!isWithin(realpathSync(root), realPath)) {
    throw new Error(`Cleanup target resolves outside the repository: ${relativePath}`)
  }
  return { absolutePath, exists: true }
}

function measureTarget(path) {
  const result = spawnSync('du', ['-sk', path], { encoding: 'utf8' })
  if (result.status !== 0) return 'size unavailable'
  const kibibytes = Number.parseInt(result.stdout.trim().split(/\s+/u)[0], 10)
  if (!Number.isFinite(kibibytes)) return 'size unavailable'
  const gibibytes = kibibytes / 1024 / 1024
  if (gibibytes >= 1) return `${gibibytes.toFixed(1)} GiB`
  const mebibytes = kibibytes / 1024
  return mebibytes >= 1 ? `${mebibytes.toFixed(1)} MiB` : `${kibibytes} KiB`
}

function systemCargoAvailable() {
  const result = spawnSync('cargo', ['--version'], { encoding: 'utf8' })
  return result.status === 0
}

function systemCargoClean(root) {
  const result = spawnSync(
    'cargo',
    ['clean', '--manifest-path', PLATFORM_MANIFEST],
    { cwd: root, stdio: 'inherit' },
  )
  if (result.status !== 0) throw new Error(`cargo clean failed with status ${result.status}`)
}

export function runCleanup({
  root,
  cwd = process.cwd(),
  apply = false,
  log = console.log,
  cargoAvailable = systemCargoAvailable,
  cargoClean = systemCargoClean,
} = {}) {
  const repositoryRoot = validateRepositoryRoot(root, cwd)
  const allTargets = [PLATFORM_TARGET, ...CLEAN_TARGETS]
  const targets = allTargets.map((path) => ({ path, ...validateTarget(repositoryRoot, path) }))
  const existingTargets = targets.filter((target) => target.exists)

  log(apply ? 'Applying local artifact cleanup:' : 'Local artifact cleanup preview:')
  if (existingTargets.length === 0) log('  No cleanup targets are present.')
  for (const target of existingTargets) {
    log(`  ${target.path} (${measureTarget(target.absolutePath)})`)
  }

  if (!apply) {
    log('Dry run only. Use `pnpm clean:local:apply` to remove these generated artifacts.')
    return { applied: false, targets: existingTargets.map(({ path }) => path) }
  }

  const platformTarget = targets.find(({ path }) => path === PLATFORM_TARGET)
  if (platformTarget?.exists && !cargoAvailable()) {
    throw new Error('Cargo is unavailable; no artifacts were removed.')
  }

  if (platformTarget?.exists) cargoClean(repositoryRoot)
  for (const target of targets) {
    if (target.path === PLATFORM_TARGET || !target.exists) continue
    rmSync(target.absolutePath, { recursive: true, force: false })
  }

  log('Local generated artifacts removed.')
  return { applied: true, targets: existingTargets.map(({ path }) => path) }
}

function parseArguments(arguments_) {
  if (arguments_.length === 0) return { apply: false }
  if (arguments_.length === 1 && arguments_[0] === '--apply') return { apply: true }
  throw new Error(`Unknown arguments: ${arguments_.join(' ')}`)
}

const modulePath = fileURLToPath(import.meta.url)
if (process.argv[1] && realpathSync(process.argv[1]) === realpathSync(modulePath)) {
  try {
    const { apply } = parseArguments(process.argv.slice(2))
    runCleanup({ root: resolve(dirname(modulePath), '../..'), apply })
  } catch (error) {
    console.error(error instanceof Error ? error.message : error)
    process.exitCode = 1
  }
}
