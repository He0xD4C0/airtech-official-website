import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs'
import { dirname, join, relative, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'
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
    if ((owner || from.startsWith('shared/')) && target.startsWith('../')) {
      errors.push(`${from}: import escapes the application source boundary: ${specifier}`)
    }
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
    if (ts.isImportTypeNode(node) && ts.isLiteralTypeNode(node.argument)
      && ts.isStringLiteral(node.argument.literal)) check(node.argument.literal.text)
    ts.forEachChild(node, visit)
  }
  visit(ast)
  return errors
}

export function checkFrontendBoundaries() {
  const failures = []
  const featureNames = {
    admin: ['auth', 'dashboard', 'content', 'catalog', 'media', 'inbox', 'analytics', 'identity', 'audit', 'integrations', 'settings', 'devtools'],
    web: ['content', 'catalog', 'compare', 'selector', 'conversion', 'search', 'analytics'],
  }
  for (const [app, expected] of Object.entries(featureNames)) {
    const root = resolve(`apps/${app}/src`)
    const actual = readdirSync(join(root, 'features')).sort()
    if (actual.join(',') !== [...expected].sort().join(',')) failures.push(`${app}: unexpected feature directory layout`)
    const allowedRoots = app === 'admin' ? ['app', 'shared', 'features'] : ['app', 'shared', 'server', 'features']
    for (const entry of readdirSync(root, { withFileTypes: true })) {
      if (entry.isDirectory() && !allowedRoots.includes(entry.name)) failures.push(`${app}: unexpected source directory ${entry.name}`)
    }
    for (const file of files(root)) {
      failures.push(...inspectImports(readFileSync(file, 'utf8'), relative(root, file), specifier => {
        const base = specifier.startsWith('@/') ? join(root, specifier.slice(2))
          : specifier.startsWith('.') ? resolve(dirname(file), specifier) : null
        if (!base) return null
        const path = [base, `${base}.ts`, `${base}.vue`, join(base, 'index.ts')]
          .find(candidate => existsSync(candidate) && statSync(candidate).isFile())
        if (!path) {
          failures.push(`${relative(root, file)}: unresolved local import: ${specifier}`)
          return null
        }
        return relative(root, path)
      }))
    }
  }
  return failures
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const failures = checkFrontendBoundaries()
  if (failures.length) {
    console.error(failures.join('\n'))
    process.exitCode = 1
  } else console.log('Frontend dependency boundaries passed.')
}
