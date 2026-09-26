import { readFileSync, readdirSync } from 'node:fs'
import { extname, join, relative, resolve } from 'node:path'

const root = resolve(import.meta.dirname, '../..')
const sourceRoots = [
  'packages/ui/src',
  'packages/content-renderer/src',
  'apps/admin/src',
]
const files = sourceRoots.flatMap((directory) => walk(resolve(root, directory)))
  .filter((file) => ['.css', '.vue'].includes(extname(file)))
const failures = []
const definedVariables = new Set()
const usedVariables = new Map()

for (const file of files) {
  const source = readFileSync(file, 'utf8')
  const label = relative(root, file)
  for (const match of source.matchAll(/(--[a-z][a-z0-9-]*)\s*:/gi)) {
    definedVariables.add(match[1])
  }
  for (const match of source.matchAll(/var\((--[a-z][a-z0-9-]*)/gi)) {
    const uses = usedVariables.get(match[1]) ?? []
    uses.push(label)
    usedVariables.set(match[1], uses)
  }
  if (/!important\b/.test(source)) failures.push(`${label}: !important is forbidden.`)
  if (/\.editor-(?:page|topbar|layout|workspace|title-fields|inspector|entity-block)\b/.test(source)) {
    failures.push(`${label}: retired .editor-* selector is forbidden.`)
  }
  for (const match of source.matchAll(/font-size\s*:\s*(0?\.\d+)rem/gi)) {
    if (Number(match[1]) < 0.75) failures.push(`${label}: font-size ${match[1]}rem is below 0.75rem.`)
  }
  for (const match of source.matchAll(/font-size\s*:\s*(\d+(?:\.\d+)?)px/gi)) {
    if (Number(match[1]) < 12) failures.push(`${label}: font-size ${match[1]}px is below 12px.`)
  }
  if (file.endsWith('.vue')) inspectVueStyles(source, label)
  if (file.includes('/packages/ui/src/') && file.endsWith('.css') && /#[0-9a-f]{6}/i.test(source)
    && !file.endsWith('/tokens.css')) {
    failures.push(`${label}: literal six-digit colors belong in tokens.css.`)
  }
}

for (const [variable, usages] of usedVariables) {
  if (!definedVariables.has(variable)) {
    failures.push(`${usages[0]}: ${variable} is used but never defined in the native CSS sources.`)
  }
}

const rootPackage = JSON.parse(readFileSync(resolve(root, 'package.json'), 'utf8'))
const packageNames = Object.keys({
  ...rootPackage.dependencies,
  ...rootPackage.devDependencies,
})
for (const forbidden of ['tailwindcss', 'unocss', 'styled-components', 'sass', 'less']) {
  if (packageNames.includes(forbidden)) failures.push(`package.json: runtime/style dependency ${forbidden} is forbidden.`)
}

if (failures.length) {
  console.error(['CSS architecture check failed:', ...failures.map((failure) => `- ${failure}`)].join('\n'))
  process.exit(1)
}

console.log(`CSS architecture check passed: ${files.length} native CSS and Vue sources inspected.`)

function inspectVueStyles(source, label) {
  for (const match of source.matchAll(/<style([^>]*)>([\s\S]*?)<\/style>/gi)) {
    const attributes = match[1]
    const css = match[2]
    if (/\blang\s*=/.test(attributes)) failures.push(`${label}: style preprocessors are forbidden.`)
    if (/\bscoped\b/.test(attributes) && !/@layer\s+components\s*\{/.test(css)) {
      failures.push(`${label}: scoped styles must live in @layer components.`)
    }
    if (/@media\s*\(/.test(css)) {
      failures.push(`${label}: viewport breakpoints belong in the Admin responsive stylesheet.`)
    }
    if (/--airtek-(?:blue|green|ink|dark-gray|muted|border|surface|white)\s*:/.test(css)) {
      failures.push(`${label}: components cannot redefine brand colors.`)
    }
  }
}

function walk(directory) {
  const results = []
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name)
    if (entry.isDirectory()) results.push(...walk(path))
    else results.push(path)
  }
  return results
}
