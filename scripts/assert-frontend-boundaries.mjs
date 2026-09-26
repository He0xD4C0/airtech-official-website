import { existsSync, readFileSync, readdirSync } from 'node:fs'
import { dirname, join, relative, resolve } from 'node:path'
import ts from 'typescript'

function files(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const path = join(directory, entry.name)
    return entry.isDirectory() ? files(path) : /\.(ts|vue)$/u.test(path) ? [path] : []
  })
}

export function inspectImports(source, from, resolveImport) {
  const errors = []
  const script = from.endsWith('.vue')
    ? [...source.matchAll(/<script[^>]*>([\s\S]*?)<\/script>/gu)].map(match => match[1]).join('\n')
    : source
  const ast = ts.createSourceFile(from, script, ts.ScriptTarget.Latest, true)
  const check = specifier => {
    const target = resolveImport(specifier)
    if (!target) return
    const owner = from.match(/^features\/([^/]+)\//u)?.[1]
    const destination = target.match(/^features\/([^/]+)\/(.*)$/u)
    if (owner && target.startsWith('app/')) errors.push(`${from}: feature imports app: ${specifier}`)
    if (from.startsWith('shared/') && /^(features|app|server)\//u.test(target)) {
      errors.push(`${from}: shared imports an upper layer: ${specifier}`)
    }
    if (owner && destination && owner !== destination[1] && destination[2] !== 'index.ts') {
      errors.push(`${from}: cross-feature import must use index.ts: ${specifier}`)
    }
    if (!from.startsWith('server/') && !from.startsWith('app/') && target.startsWith('server/')) {
      errors.push(`${from}: browser code imports server code: ${specifier}`)
    }
  }
  function visit(node) {
    if ((ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) && node.moduleSpecifier) {
      check(node.moduleSpecifier.text)
    }
    if (ts.isCallExpression(node) && node.expression.kind === ts.SyntaxKind.ImportKeyword
      && node.arguments[0] && ts.isStringLiteral(node.arguments[0])) check(node.arguments[0].text)
    ts.forEachChild(node, visit)
  }
  visit(ast)
  return errors
}

const failures = []
for (const app of ['admin', 'web']) {
  const root = resolve(`apps/${app}/src`)
  if (!existsSync(join(root, 'features'))) continue
  for (const file of files(root)) {
    failures.push(...inspectImports(readFileSync(file, 'utf8'), relative(root, file), specifier => {
      const base = specifier.startsWith('@/') ? join(root, specifier.slice(2))
        : specifier.startsWith('.') ? resolve(dirname(file), specifier) : null
      if (!base) return null
      const path = [base, `${base}.ts`, `${base}.vue`, join(base, 'index.ts')]
        .find(candidate => existsSync(candidate) && !readdirSafe(candidate))
      if (!path) {
        failures.push(`${relative(root, file)}: unresolved local import: ${specifier}`)
        return null
      }
      return relative(root, path)
    }))
  }
}
function readdirSafe(path) {
  try { readdirSync(path); return true } catch { return false }
}
if (failures.length) {
  console.error(failures.join('\n'))
  process.exitCode = 1
} else console.log('Frontend dependency boundaries passed.')
