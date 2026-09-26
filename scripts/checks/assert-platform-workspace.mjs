import { execFileSync } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

export const layers = {
  'airtek-domain': [],
  'airtek-runtime': ['airtek-domain'],
  'airtek-http': ['airtek-domain', 'airtek-runtime'],
  'airtek-jobs': ['airtek-domain', 'airtek-runtime'],
  'airtek-api': ['airtek-runtime', 'airtek-http'],
  'airtek-worker': ['airtek-runtime', 'airtek-jobs'],
  'airtek-maintenance': ['airtek-runtime'],
  'airtek-platform-contract-tests': ['airtek-domain', 'airtek-runtime', 'airtek-http', 'airtek-jobs'],
}

export function validateLayers(packages) {
  const failures = []
  for (const [name, allowed] of Object.entries(layers)) {
    const member = packages.find((item) => item.name === name)
    if (!member) { failures.push(`Missing workspace member: ${name}`); continue }
    for (const dependency of member.dependencies) {
      if (dependency.name.startsWith('airtek-') && !allowed.includes(dependency.name)) {
        failures.push(`${name} must not depend on ${dependency.name}`)
      }
    }
    if (name === 'airtek-domain' && member.dependencies.some((dep) => /^(axum|sqlx|tokio|reqwest)$/.test(dep.name))) {
      failures.push('Domain must remain free of HTTP, database and async runtime dependencies')
    }
    for (const feature of name === 'airtek-domain' ? [] : ['production', 'devtools']) {
      if (!Object.hasOwn(member.features, feature)) failures.push(`${name} must declare ${feature}`)
      for (const dependency of member.dependencies.filter((dep) => allowed.includes(dep.name) && dep.name !== 'airtek-domain')) {
        if (!member.features[feature]?.includes(`${dependency.name}/${feature}`)) {
          failures.push(`${name} must forward ${feature} to ${dependency.name}`)
        }
      }
    }
  }
  for (const member of packages) {
    if (!Object.hasOwn(layers, member.name)) failures.push(`Unexpected workspace member: ${member.name}`)
  }
  return failures
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--manifest-path', 'services/platform/Cargo.toml', '--format-version', '1', '--no-deps', '--locked'], { encoding: 'utf8' }))
  const failures = validateLayers(metadata.packages)
  if (existsSync('services/platform/src')) failures.push('Retired monolith source directory must not exist')
  if (!/resolver\s*=\s*"2"/.test(readFileSync('services/platform/Cargo.toml', 'utf8'))) failures.push('Workspace must use resolver 2')
  if (failures.length) throw new Error(failures.join('\n'))
  console.log('Platform workspace boundaries passed: 8 members with one-way dependencies.')
}
