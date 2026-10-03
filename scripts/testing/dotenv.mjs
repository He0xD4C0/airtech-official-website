import { existsSync, readFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..')

// Compose reads the repository-root .env automatically; the test runners do the
// same so a developer fills the private object-storage image references once.
// Existing process values always win, matching Compose precedence.
export function repositoryDotenv(source = process.env) {
  const file = join(repositoryRoot, '.env')
  if (!existsSync(file)) return {}
  const values = {}
  for (const line of readFileSync(file, 'utf8').split(/\r?\n/u)) {
    const match = /^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=(.*)$/u.exec(line)
    if (!match) continue
    const [, key, rawValue] = match
    if (source[key] !== undefined) continue
    let value = rawValue.trim()
    if (value.length >= 2 && ((value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'")))) {
      value = value.slice(1, -1)
    }
    values[key] = value
  }
  return values
}
