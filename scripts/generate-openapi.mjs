import { execFileSync } from 'node:child_process'
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, relative, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'

import {
  compareGeneratedTree,
  writeModularOpenApiTypes,
} from './openapi-typescript-modules.mjs'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const snapshot = join(root, 'packages/contracts/openapi/openapi.production.json')
const generatedRoot = join(root, 'packages/contracts/src/generated')
const check = process.argv.includes('--check')
const scratch = mkdtempSync(join(tmpdir(), 'airtek-openapi-'))
const candidateSnapshot = check ? join(scratch, 'openapi.production.json') : snapshot
const rawGenerated = join(scratch, 'openapi.raw.ts')
const candidateGeneratedRoot = check ? join(scratch, 'generated') : generatedRoot
const HTTP_METHODS = new Set(['get', 'put', 'post', 'delete', 'options', 'head', 'patch', 'trace'])

try {
  const cargo = resolveCargo()
  const raw = execFileSync(cargo.command, [
    ...cargo.arguments,
    'run',
    '--quiet',
    '--locked',
    '--manifest-path',
    'services/platform/Cargo.toml',
    '--example',
    'export_openapi',
    '--features',
    'production',
  ], {
    cwd: root,
    encoding: 'utf8',
    env: { ...process.env, ...cargo.environment },
    stdio: ['ignore', 'pipe', 'inherit'],
  })
  const document = JSON.parse(raw)
  assertProductionDocument(document)
  assertRustRouteInventory(document)
  assertRequiredDataContracts(document)
  mkdirSync(dirname(candidateSnapshot), { recursive: true })
  mkdirSync(dirname(rawGenerated), { recursive: true })
  writeFileSync(candidateSnapshot, `${JSON.stringify(document, null, 2)}\n`)

  const openApiTypescriptCli = join(root, 'node_modules/openapi-typescript/bin/cli.js')
  execFileSync(process.execPath, [
    openApiTypescriptCli,
    candidateSnapshot,
    '--output',
    rawGenerated,
    '--alphabetize',
    '--default-non-nullable',
    'false',
    '--root-types',
    '--root-types-no-schema-prefix',
  ], {
    cwd: root,
    stdio: 'inherit',
  })
  writeModularOpenApiTypes(rawGenerated, candidateGeneratedRoot)

  if (check) {
    compare(snapshot, candidateSnapshot, 'OpenAPI snapshot')
    compareGeneratedTree(generatedRoot, candidateGeneratedRoot, ['openapi/runtime-data.ts'])
    console.log('OpenAPI snapshot and generated TypeScript contract are current.')
  } else {
    console.log('Updated production OpenAPI snapshot and generated TypeScript contract.')
  }
} finally {
  rmSync(scratch, { recursive: true, force: true })
}

function assertProductionDocument(document) {
  if (document?.['x-airtek-build'] !== 'production') {
    throw new Error('Contract export must be compiled with the production feature.')
  }
  if (typeof document?.openapi !== 'string' || !document.openapi.startsWith('3.')) {
    throw new Error('Contract export must be an OpenAPI 3 document.')
  }
  for (const path of Object.keys(document.paths || {})) {
    if (path.startsWith('/api/devtools/')) {
      throw new Error(`Production OpenAPI contract contains the DevTools route ${path}.`)
    }
  }
  const forbiddenIdentifier = /(devtools|terminal|pty|shellsession)/i
  for (const schemaName of Object.keys(document?.components?.schemas || {})) {
    if (forbiddenIdentifier.test(schemaName)) {
      throw new Error(`Production OpenAPI contract contains the DevTools schema ${schemaName}.`)
    }
  }
  const operationIds = new Map()
  for (const [path, pathItem] of Object.entries(document.paths || {})) {
    for (const [method, operation] of Object.entries(pathItem)) {
      if (!HTTP_METHODS.has(method) || !operation || typeof operation !== 'object') continue
      const operationId = operation.operationId
      if (typeof operationId !== 'string' || !operationId) {
        throw new Error(`${method.toUpperCase()} ${path} is missing operationId.`)
      }
      if (forbiddenIdentifier.test(operationId)
        || (operation.tags || []).some((tag) => forbiddenIdentifier.test(String(tag)))) {
        throw new Error(`Production OpenAPI operation ${operationId} exposes DevTools metadata.`)
      }
      if (operationIds.has(operationId)) {
        throw new Error(
          `Duplicate operationId ${operationId}: ${operationIds.get(operationId)} and ${method} ${path}.`,
        )
      }
      operationIds.set(operationId, `${method} ${path}`)
    }
  }
  if (!document?.components?.schemas?.ProblemDetails) {
    throw new Error('ProblemDetails is missing from the production OpenAPI contract.')
  }
}

function assertRequiredDataContracts(document) {
  const operations = [
    ['post', '/api/admin/v1/auth/invitations/accept', 'acceptAdministratorInvitation'],
    ['get', '/api/admin/v1/content-drafts/{draftId}', 'getPrivateContentDraft'],
    ['patch', '/api/admin/v1/content-drafts/{draftId}', 'savePrivateContentDraft'],
    ['post', '/api/admin/v1/content-drafts/{draftId}/submit', 'submitPrivateContentDraft'],
    ['get', '/api/admin/v1/content-reviews', 'listContentReviews'],
    ['post', '/api/admin/v1/content-reviews/{draftId}/approve', 'approveContentReview'],
    ['get', '/api/admin/v1/published-content/{contentId}', 'getCurrentPublishedContent'],
    ['post', '/api/admin/v1/published-content/{contentId}/drafts', 'copyPublishedContentToPrivateDraft'],
    ['get', '/api/admin/v1/site-singletons/{kind}', 'getSiteSingletonState'],
    ['get', '/api/admin/v1/products/{id}/private-pricing', 'getProductPrivatePricing'],
  ]
  for (const [method, path, operationId] of operations) {
    if (document?.paths?.[path]?.[method]?.operationId !== operationId) {
      throw new Error(`Required production contract ${method.toUpperCase()} ${path} is missing.`)
    }
  }

  const requiredSchemaProperties = {
    Product: ['seo', 'sortOrder', 'relatedContentIds', 'mediaGallery'],
    ProductPresentation: ['seo', 'sortOrder', 'relatedContentIds', 'mediaGallery'],
    UpdateProductPresentation: ['seo', 'sortOrder', 'relatedContentIds', 'mediaGallery'],
    ProductPrivatePricing: ['productId', 'stableId', 'sourceRowNumber', 'pricingFields'],
    GuestSourceDaily: ['bucketDate', 'source', 'landingPath', 'locale', 'visits', 'pageViews', 'rfqStarts', 'rfqSubmissions'],
    ContentDraftV2: ['schemaVersion', 'kind', 'templateKey', 'isPlaceholder', 'typeFields', 'composition', 'seo', 'relations', 'draftVersion'],
    CmsPrivateDraft: ['draftId', 'contentId', 'ownerUserId', 'document', 'draftVersion', 'basePublicationVersion', 'state'],
    CmsPublishedContent: ['contentId', 'document', 'publicationVersion', 'publishedBy', 'publishedAt'],
    CmsReviewItem: ['draft', 'submittedByUserId', 'submittedAt'],
    CmsSiteSingletonState: ['ownDraft', 'published'],
    GeneralInformationTypeFields: ['contact', 'socialLinks', 'defaultSeo', 'productCategories', 'navigationCta'],
  }
  for (const [schemaName, properties] of Object.entries(requiredSchemaProperties)) {
    const schema = document?.components?.schemas?.[schemaName]
    if (!schema) throw new Error(`Required production schema ${schemaName} is missing.`)
    const schemaProperties = schema.properties || {}
    for (const property of properties) {
      if (!(property in schemaProperties)) {
        throw new Error(`Required production schema property ${schemaName}.${property} is missing.`)
      }
    }
  }
  const schemas = document.components.schemas
  if (schemas.ContentBlock?.discriminator?.propertyName !== 'type'
    || schemas.ContentTypeFields?.discriminator?.propertyName !== 'type') {
    throw new Error('CMS V2 discriminators must use the Rust serde `type` property.')
  }
  if ('canonicalPath' in (schemas.SeoInputV2?.properties || {})) {
    throw new Error('CMS V2 canonical paths are server-derived, not editable SEO input.')
  }
  if ((schemas.CmsPublicationStatusV2?.enum || []).includes('scheduled')) {
    throw new Error('CMS V2 must not expose deferred scheduled publishing.')
  }
  if ((schemas.ContentTemplateKey?.enum || []).includes('productDetail')) {
    throw new Error('Product detail is catalog-owned and cannot be a CMS V2 template.')
  }
}

function assertRustRouteInventory(document) {
  const roots = [
    { relativePath: 'services/platform/src/routes/system.rs', prefix: '' },
    { relativePath: 'services/platform/src/routes/public.rs', prefix: '/api/public/v1' },
    { relativePath: 'services/platform/src/auth.rs', prefix: '/api/admin/v1' },
    { relativePath: 'services/platform/src/routes/admin.rs', prefix: '/api/admin/v1' },
  ]
  const registered = new Map()
  const visited = new Set()
  for (const source of roots) {
    collectRouterOperations(source.relativePath, source.prefix, registered, visited)
  }
  assertEveryProductionRouteModuleIsReachable(visited)

  const documented = new Set()
  for (const [path, pathItem] of Object.entries(document.paths || {})) {
    for (const method of Object.keys(pathItem)) {
      if (HTTP_METHODS.has(method)) documented.add(`${method} ${path}`)
    }
  }
  const missing = [...registered.keys()].filter((operation) => !documented.has(operation)).sort()
  const stale = [...documented].filter((operation) => !registered.has(operation)).sort()
  if (missing.length || stale.length) {
    throw new Error([
      'Rust router and production OpenAPI operation inventory differ.',
      missing.length
        ? `Missing from OpenAPI: ${missing.map((operation) => `${operation} (${registered.get(operation)})`).join(', ')}`
        : '',
      stale.length ? `Not registered by Rust: ${stale.join(', ')}` : '',
    ].filter(Boolean).join('\n'))
  }
}

function collectRouterOperations(relativePath, prefix, registered, visited) {
  const normalizedPath = normalizeRelativePath(relativePath)
  const visitKey = `${normalizedPath}\0${prefix}`
  if (visited.has(visitKey)) return
  visited.add(visitKey)

  const sourcePath = join(root, normalizedPath)
  if (!existsSync(sourcePath)) {
    throw new Error(`Router module ${normalizedPath} does not exist.`)
  }
  const source = readRustSource(sourcePath)
  const router = extractRustFunction(source, 'router', normalizedPath)
  const routeMatches = [...router.matchAll(/\.route\(\s*"([^"]+)"\s*,/g)]
  for (let index = 0; index < routeMatches.length; index += 1) {
    const match = routeMatches[index]
    const next = routeMatches[index + 1]
    const segment = router.slice(match.index, next?.index ?? router.length)
    const methods = [...segment.matchAll(/\b(get|post|put|patch|delete|options|head|trace)\s*\(/g)]
    if (!methods.length) {
      throw new Error(`Unable to identify an HTTP method for ${normalizedPath}:${match[1]}.`)
    }
    for (const method of methods) {
      const operation = `${method[1]} ${joinUrlPath(prefix, match[1])}`
      const prior = registered.get(operation)
      if (prior) {
        throw new Error(`Rust route ${operation} is registered more than once (${prior}, ${normalizedPath}).`)
      }
      registered.set(operation, normalizedPath)
    }
  }

  for (const match of router.matchAll(/\.merge\(\s*([A-Za-z0-9_:]+)::router\(\)\s*\)/g)) {
    const mergedPath = resolveRustModule(normalizedPath, match[1])
    collectRouterOperations(mergedPath, prefix, registered, visited)
  }
}

function extractRustFunction(source, functionName, relativePath) {
  const signature = new RegExp(`pub\\s+fn\\s+${functionName}\\s*\\(\\s*\\)`)
  const match = signature.exec(source)
  if (!match) throw new Error(`Unable to find ${functionName}() in ${relativePath}.`)
  const openBrace = source.indexOf('{', match.index + match[0].length)
  if (openBrace < 0) throw new Error(`Unable to find the body of ${functionName}() in ${relativePath}.`)
  const closeBrace = findMatchingRustBrace(source, openBrace)
  if (closeBrace < 0) throw new Error(`Unable to parse the body of ${functionName}() in ${relativePath}.`)
  return source.slice(openBrace + 1, closeBrace)
}

function readRustSource(sourcePath, visited = new Set()) {
  const normalized = resolve(sourcePath)
  if (visited.has(normalized)) {
    throw new Error(`Rust include cycle detected at ${normalizeRelativePath(relative(root, normalized))}.`)
  }
  const nextVisited = new Set(visited).add(normalized)
  return readFileSync(normalized, 'utf8')
    .replace(
      /#\[path\s*=\s*"([^"]+)"\]\s*mod\s+[A-Za-z_][A-Za-z0-9_]*\s*;/g,
      (_match, modulePath) => readRustSource(resolve(dirname(normalized), modulePath), nextVisited),
    )
    .replace(
      /include!\(\s*"([^"]+)"\s*\);/g,
      (_match, includedPath) => readRustSource(resolve(dirname(normalized), includedPath), nextVisited),
    )
}

function findMatchingRustBrace(source, openBrace) {
  let depth = 0
  let state = 'code'
  let blockCommentDepth = 0
  for (let index = openBrace; index < source.length; index += 1) {
    const character = source[index]
    const next = source[index + 1]
    if (state === 'lineComment') {
      if (character === '\n') state = 'code'
      continue
    }
    if (state === 'blockComment') {
      if (character === '/' && next === '*') {
        blockCommentDepth += 1
        index += 1
      } else if (character === '*' && next === '/') {
        blockCommentDepth -= 1
        index += 1
        if (blockCommentDepth === 0) state = 'code'
      }
      continue
    }
    if (state === 'string' || state === 'character') {
      if (character === '\\') {
        index += 1
      } else if ((state === 'string' && character === '"')
        || (state === 'character' && character === "'")) {
        state = 'code'
      }
      continue
    }
    if (character === '/' && next === '/') {
      state = 'lineComment'
      index += 1
    } else if (character === '/' && next === '*') {
      state = 'blockComment'
      blockCommentDepth = 1
      index += 1
    } else if (character === '"') {
      state = 'string'
    } else if (character === "'") {
      state = 'character'
    } else if (character === '{') {
      depth += 1
    } else if (character === '}') {
      depth -= 1
      if (depth === 0) return index
    }
  }
  return -1
}

function resolveRustModule(fromRelativePath, moduleReference) {
  if (moduleReference === 'crate::auth') return 'services/platform/src/auth.rs'

  let modulePath
  if (moduleReference.startsWith('crate::routes::')) {
    modulePath = join(
      root,
      'services/platform/src/routes',
      ...moduleReference.slice('crate::routes::'.length).split('::'),
    )
  } else if (moduleReference.startsWith('super::')) {
    modulePath = join(
      root,
      dirname(fromRelativePath),
      ...moduleReference.slice('super::'.length).split('::'),
    )
  } else if (moduleReference.startsWith('self::')) {
    modulePath = join(
      root,
      dirname(fromRelativePath),
      ...moduleReference.slice('self::'.length).split('::'),
    )
  } else {
    modulePath = join(root, dirname(fromRelativePath), ...moduleReference.split('::'))
  }

  const candidates = [`${modulePath}.rs`, join(modulePath, 'mod.rs')]
  const resolved = candidates.find((candidate) => existsSync(candidate))
  if (!resolved) {
    throw new Error(
      `Unable to resolve merged Rust router module ${moduleReference} from ${fromRelativePath}.`,
    )
  }
  return normalizeRelativePath(relative(root, resolved))
}

function assertEveryProductionRouteModuleIsReachable(visited) {
  const routeRoot = join(root, 'services/platform/src/routes')
  const visitedPaths = new Set([...visited].map((entry) => entry.split('\0')[0]))
  for (const absolutePath of walkRustFiles(routeRoot)) {
    const relativePath = normalizeRelativePath(relative(root, absolutePath))
    if (relativePath.endsWith('/mod.rs')) continue
    const source = readFileSync(absolutePath, 'utf8')
    if (!/pub\s+fn\s+router\s*\(\s*\)/.test(source)) continue
    if (isDevelopmentOnlyRouter(relativePath, source)) continue
    if (!visitedPaths.has(relativePath)) {
      throw new Error(
        `Production router module ${relativePath} is not reachable from the contract inventory roots.`,
      )
    }
  }
}

function walkRustFiles(directory) {
  const files = []
  for (const entry of readdirSync(directory)) {
    const absolutePath = join(directory, entry)
    if (statSync(absolutePath).isDirectory()) files.push(...walkRustFiles(absolutePath))
    else if (entry.endsWith('.rs')) files.push(absolutePath)
  }
  return files
}

function isDevelopmentOnlyRouter(relativePath, source) {
  return /(^|\/)(devtools?|development)(\/|_|\.|$)/i.test(relativePath)
    || /#\s*\[\s*cfg\s*\(\s*feature\s*=\s*"devtools"\s*\)\s*\][\s\S]{0,160}pub\s+fn\s+router/.test(source)
}

function joinUrlPath(prefix, routePath) {
  const joined = `${prefix}/${routePath}`.replaceAll(/\/{2,}/g, '/')
  return joined.length > 1 && joined.endsWith('/') ? joined.slice(0, -1) : joined
}

function normalizeRelativePath(path) {
  return path.split(sep).join('/')
}

function compare(expectedPath, actualPath, label) {
  if (!existsSync(expectedPath)) {
    throw new Error(`${label} is missing. Run pnpm generate:contracts.`)
  }
  const expected = readFileSync(expectedPath, 'utf8').replaceAll('\r\n', '\n')
  const actual = readFileSync(actualPath, 'utf8').replaceAll('\r\n', '\n')
  if (expected !== actual) {
    throw new Error(`${label} drift detected. Run pnpm generate:contracts and commit the result.`)
  }
}

function resolveCargo() {
  if (process.env.CARGO) {
    return { command: process.env.CARGO, arguments: [], environment: {} }
  }
  try {
    execFileSync('cargo', ['--version'], { stdio: 'ignore' })
    return { command: 'cargo', arguments: [], environment: {} }
  } catch (error) {
    if (error?.code !== 'ENOENT') throw error
  }
  try {
    const tool = (name) => execFileSync('rustup', ['which', name], {
      cwd: root,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'inherit'],
    }).trim()
    const cargo = tool('cargo')
    const rustc = tool('rustc')
    const rustdoc = tool('rustdoc')
    if (cargo && rustc) {
      return {
        command: cargo,
        arguments: [],
        environment: { RUSTC: rustc, ...(rustdoc ? { RUSTDOC: rustdoc } : {}) },
      }
    }
  } catch (error) {
    if (error?.code !== 'ENOENT') throw error
  }
  throw new Error('Cargo was not found. Install Cargo or set CARGO to its executable path.')
}
