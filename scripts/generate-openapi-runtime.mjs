import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const snapshot = resolve(root, 'packages/contracts/openapi/openapi.production.json')
const output = resolve(root, 'packages/contracts/src/generated/openapi/runtime-data.ts')
const check = process.argv.includes('--check')
const document = JSON.parse(readFileSync(snapshot, 'utf8'))

const schemaLines = Object.entries(document.components?.schemas ?? {})
  .sort(([left], [right]) => left.localeCompare(right))
  .map(([name, schema]) => `  ${JSON.stringify(name)}: ${JSON.stringify(schema)},`)
const responseLines = []
for (const [path, pathItem] of Object.entries(document.paths ?? {})) {
  for (const [method, operation] of Object.entries(pathItem)) {
    if (!operation?.responses) continue
    for (const [status, response] of Object.entries(operation.responses)) {
      if (!String(status).startsWith('2')) continue
      const schema = response?.content?.['application/json']?.schema
        ?? response?.content?.['application/problem+json']?.schema
      if (schema) responseLines.push(
        `  ${JSON.stringify(`${method} ${path} ${status}`)}: ${JSON.stringify(schema)},`,
      )
    }
  }
}
responseLines.sort()

const source = `/** Auto-generated from the production OpenAPI document. */
import type { OpenApiRuntimeSchema } from '../../runtime'

export const openApiSchemas: Record<string, OpenApiRuntimeSchema> = {
${schemaLines.join('\n')}
}

export const openApiResponseSchemas: Record<string, OpenApiRuntimeSchema> = {
${responseLines.join('\n')}
}
`

if (source.split('\n').length > 500) throw new Error('Runtime parser data exceeds 500 lines.')
if (check) {
  const current = existsSync(output) ? readFileSync(output, 'utf8').replaceAll('\r\n', '\n') : ''
  if (current !== source) {
    throw new Error('Generated OpenAPI runtime parser data is stale.')
  }
  console.log('Generated OpenAPI runtime parser data is current.')
} else {
  writeFileSync(output, source)
  console.log('Updated generated OpenAPI runtime parser data.')
}
