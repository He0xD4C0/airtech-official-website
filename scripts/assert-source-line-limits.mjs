import { createHash } from 'node:crypto'
import { existsSync, readdirSync, readFileSync } from 'node:fs'
import { extname, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

export const MAX_SOURCE_LINES = 500
export const SOURCE_ROOTS = ['.']
export const SOURCE_EXTENSIONS = new Set([
  '.cjs',
  '.css',
  '.html',
  '.js',
  '.jsx',
  '.mjs',
  '.py',
  '.rs',
  '.sh',
  '.sql',
  '.ts',
  '.tsx',
  '.vue',
])
export const SKIPPED_DIRECTORIES = new Set([
  '.git',
  '.pnpm-store',
  '.prebuilt',
  'coverage',
  'dist',
  'docs',
  'node_modules',
  'playwright-report',
  'target',
  'test-results',
  'tmp',
])

export const GRANDFATHERED_SOURCES = new Map([
  [
    'services/platform/migrations/V0001__platform_foundation.sql',
    {
      lines: 522,
      sha256: '88aed51095ae810060cb74ab7f32d35d91277813e1c22d22fa5a7e984edaf9e3',
    },
  ],
  [
    'services/platform/migrations/V0005__operational_data_architecture.sql',
    {
      lines: 812,
      sha256: '61d3526eb5e98203b150056ffb436da0db9df116d4ec432cd6e2e6a8380481a7',
    },
  ],
])

export function logicalLineCount(content) {
  if (!content) return 0
  const lines = content.split(/\r?\n/u)
  if (lines.at(-1) === '') lines.pop()
  return lines.length
}

export function sha256(content) {
  return createHash('sha256').update(content).digest('hex')
}

function sourceFiles(directory, options) {
  if (!existsSync(directory)) return []
  const files = []
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.isDirectory() && options.skippedDirectories.has(entry.name)) continue
    const path = join(directory, entry.name)
    if (entry.isDirectory()) files.push(...sourceFiles(path, options))
    else if (options.sourceExtensions.has(extname(entry.name))) files.push(path)
  }
  return files
}

export function auditSourceLines(root, overrides = {}) {
  const options = {
    grandfatheredSources: overrides.grandfatheredSources ?? GRANDFATHERED_SOURCES,
    maxSourceLines: overrides.maxSourceLines ?? MAX_SOURCE_LINES,
    skippedDirectories: overrides.skippedDirectories ?? SKIPPED_DIRECTORIES,
    sourceExtensions: overrides.sourceExtensions ?? SOURCE_EXTENSIONS,
    sourceRoots: overrides.sourceRoots ?? SOURCE_ROOTS,
  }
  const violations = []
  const inspected = options.sourceRoots.flatMap((directory) =>
    sourceFiles(join(root, directory), options),
  )

  for (const path of inspected) {
    const repositoryPath = relative(root, path).split('\\').join('/')
    const content = readFileSync(path)
    const lines = logicalLineCount(content.toString('utf8'))
    const exception = options.grandfatheredSources.get(repositoryPath)
    if (exception) {
      const digest = sha256(content)
      if (lines !== exception.lines || digest !== exception.sha256) {
        violations.push({
          detail: `immutable exception changed; expected ${exception.lines} lines and sha256 ${exception.sha256}, found ${lines} lines and sha256 ${digest}`,
          path: repositoryPath,
        })
      }
    } else if (lines > options.maxSourceLines) {
      violations.push({
        detail: `${lines} lines exceeds the ${options.maxSourceLines}-line limit`,
        path: repositoryPath,
      })
    }
  }

  for (const [repositoryPath] of options.grandfatheredSources) {
    if (!inspected.some((path) => relative(root, path).split('\\').join('/') === repositoryPath)) {
      violations.push({ detail: 'pinned immutable exception is missing', path: repositoryPath })
    }
  }

  return { inspected: inspected.length, violations }
}

export function run(root = process.cwd()) {
  const result = auditSourceLines(resolve(root))
  if (result.violations.length) {
    throw new Error(`Source line-limit violations:\n${result.violations
      .map(({ path, detail }) => `- ${path}: ${detail}`)
      .join('\n')}`)
  }
  console.log(
    `Source line-limit check passed: ${result.inspected} files inspected; all are at or below ${MAX_SOURCE_LINES} lines except the two pinned immutable Flyway migrations.`,
  )
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) run()
