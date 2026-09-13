import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const openApiPath = resolve(root, 'packages/contracts/openapi/openapi.production.json')
const matrixPath = resolve(root, 'packages/contracts/admin-operation-matrix.json')
const check = process.argv.includes('--check')
const methods = ['get', 'post', 'put', 'patch', 'delete']

const document = JSON.parse(readFileSync(openApiPath, 'utf8'))
const matrix = Object.entries(document.paths ?? {})
  .filter(([path]) => path.startsWith('/api/admin/v1/'))
  .flatMap(([path, pathItem]) => methods.flatMap((method) => {
    const operation = pathItem[method]
    return operation ? [describeOperation(path, method, operation)] : []
  }))
  .sort((left, right) => left.path.localeCompare(right.path) || left.method.localeCompare(right.method))

assertComplete(matrix)
const output = `${JSON.stringify({
  generatedFrom: 'packages/contracts/openapi/openapi.production.json',
  operationCount: matrix.length,
  operations: matrix,
}, null, 2)}\n`

if (check) {
  if (!existsSync(matrixPath) || readFileSync(matrixPath, 'utf8') !== output) {
    throw new Error('Admin operation matrix is stale. Run pnpm generate:contracts.')
  }
  console.log(`Admin operation matrix is current (${matrix.length} operations).`)
} else {
  writeFileSync(matrixPath, output)
  console.log(`Updated Admin operation matrix (${matrix.length} operations).`)
}

function describeOperation(path, method, operation) {
  const parameters = [...(operation.parameters ?? [])]
  const parameterNames = parameters.map((parameter) => parameter.name).filter(Boolean)
  const requestBody = Object.entries(operation.requestBody?.content ?? {}).map(([mediaType, content]) => ({
    mediaType,
    type: schemaName(content?.schema),
  }))
  const successResponses = Object.entries(operation.responses ?? {})
    .filter(([status]) => /^2\d\d$/.test(status))
    .map(([status, response]) => ({
      status: Number(status),
      types: Object.entries(response?.content ?? {}).map(([mediaType, content]) => ({
        mediaType,
        type: schemaName(content?.schema),
      })),
    }))
  return {
    method: method.toUpperCase(),
    path,
    operationId: operation.operationId,
    permission: permissionFor(path, method),
    request: {
      parameters: parameterNames,
      body: requestBody,
    },
    response: successResponses,
    pagination: parameterNames.includes('cursor')
      ? { strategy: 'cursor', cursor: 'cursor', limit: 'limit' }
      : { strategy: 'none' },
    idempotency: parameterNames.includes('Idempotency-Key')
      ? { required: true, header: 'Idempotency-Key' }
      : { required: false },
    concurrency: parameterNames.includes('If-Match')
      ? { required: true, header: 'If-Match' }
      : { required: false },
    audit: auditFor(path, method, operation.operationId),
    frontendConsumer: frontendConsumerFor(path),
  }
}

function schemaName(schema) {
  if (!schema) return 'none'
  if (schema.$ref) return schema.$ref.split('/').at(-1)
  if (schema.type === 'array') return `Array<${schemaName(schema.items)}>`
  return schema.type ?? 'inline'
}

function permissionFor(path, method) {
  const write = method !== 'get'
  if (path.startsWith('/api/admin/v1/auth/')) return 'authenticated-or-public-auth-flow'
  if (path === '/api/admin/v1/dashboard/summary') return 'authenticated; metrics filtered by caller permissions'
  if (path === '/api/admin/v1/media/assets' && method === 'get') return 'content.read | media.write'
  if (path.includes('/media/')) return write ? 'media.write' : 'content.read'
  if (path.endsWith('/content') || path.includes('/content/')) {
    if (path.endsWith('/publish') || path.endsWith('/unpublish')) return 'content.publish'
    return write ? 'content.write' : 'content.read'
  }
  if (path.endsWith('/products') || path.includes('/products/')) {
    if (path.endsWith('/private-pricing')) return 'product.pricing.read'
    if (path.endsWith('/publish')) return 'product.publish'
    return write ? 'product.write' : 'product.read'
  }
  if (path.includes('/feishu/')) return 'integration.run'
  if ((path.includes('/rfqs/') || path.includes('/contacts/')) && path.endsWith('/pii')) return 'rfq.read_pii'
  if (path.includes('/rfqs') || path.includes('/contacts')) return write ? 'rfq.assign' : 'rfq.read'
  if (path.includes('/analytics/')) return 'analytics.read'
  if (path.includes('/user-invitations') || path.includes('/users') || path.includes('/roles')) return 'identity.manage'
  if (path.includes('/settings')) return 'settings.manage'
  if (path.includes('/operations')) return 'operations.run'
  if (path.includes('/audit')) return 'audit.read'
  throw new Error(`No Admin permission policy declared for ${method.toUpperCase()} ${path}.`)
}

function auditFor(path, method, operationId) {
  if (path.endsWith('/pii')) return { required: true, action: `${operationId}.read` }
  if (method === 'get') return { required: false }
  if (path.startsWith('/api/admin/v1/auth/login') || path.startsWith('/api/admin/v1/auth/session')) {
    return { required: false }
  }
  return { required: true, action: operationId }
}

function frontendConsumerFor(path) {
  if (path.includes('/auth/')) return 'apps/admin/src/services/adminAuthApi.ts'
  if (path.includes('/content')) return 'apps/admin/src/services/contentApi.ts'
  if (path.includes('/products')) return 'apps/admin/src/services/adminProductApi.ts'
  if (path.includes('/media/')) return 'apps/admin/src/services/mediaApi.ts'
  if (path.includes('/feishu/')) return 'apps/admin/src/services/adminIntegrationApi.ts'
  if (path.includes('/rfqs') || path.includes('/contacts') || path.includes('/dashboard')) {
    return 'apps/admin/src/services/adminEngagementApi.ts'
  }
  if (path.includes('/analytics/')) return 'apps/admin/src/services/adminApi.ts'
  if (path.includes('/users') || path.includes('/roles') || path.includes('/user-invitations')) {
    return 'apps/admin/src/services/adminIdentityApi.ts'
  }
  return 'apps/admin/src/services/adminOperationsApi.ts'
}

function assertComplete(operations) {
  if (!operations.length) throw new Error('Admin operation matrix is empty.')
  const operationIds = new Set()
  for (const operation of operations) {
    for (const field of ['method', 'path', 'operationId', 'permission', 'request', 'response', 'pagination', 'idempotency', 'concurrency', 'audit', 'frontendConsumer']) {
      if (operation[field] === undefined || operation[field] === '') {
        throw new Error(`${operation.method} ${operation.path} is missing matrix field ${field}.`)
      }
    }
    if (!operation.response.length) throw new Error(`${operation.operationId} has no success response.`)
    if (operationIds.has(operation.operationId)) throw new Error(`Duplicate operationId ${operation.operationId}.`)
    operationIds.add(operation.operationId)
  }
}
