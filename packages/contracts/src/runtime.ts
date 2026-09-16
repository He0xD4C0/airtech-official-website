import { openApiResponseSchemas, openApiSchemas } from './generated/openapi/runtime-data'

export type OpenApiRuntimeSchema = boolean | {
  [keyword: string]: unknown
  $ref?: string
  type?: string | string[]
  enum?: unknown[]
  const?: unknown
  oneOf?: OpenApiRuntimeSchema[]
  anyOf?: OpenApiRuntimeSchema[]
  allOf?: OpenApiRuntimeSchema[]
  required?: string[]
  properties?: Record<string, OpenApiRuntimeSchema>
  additionalProperties?: boolean | OpenApiRuntimeSchema
  items?: OpenApiRuntimeSchema
  minLength?: number
  maxLength?: number
  minimum?: number
  maximum?: number
  pattern?: string
}

export class OpenApiParseError extends TypeError {
  constructor(public readonly issues: string[]) {
    super(`OpenAPI response validation failed: ${issues.join('; ')}`)
    this.name = 'OpenApiParseError'
  }
}

export function parseOpenApiSchema<T>(schemaName: string, value: unknown): T {
  const schema = openApiSchemas[schemaName]
  if (!schema) throw new TypeError(`Unknown OpenAPI schema: ${schemaName}`)
  const issues: string[] = []
  validate(schema, value, '$', issues)
  if (issues.length) throw new OpenApiParseError(issues)
  return value as T
}

export function parseOpenApiResponse(
  method: string,
  pathTemplate: string,
  status: number,
  value: unknown,
): unknown {
  const schema = openApiResponseSchemas[`${method.toLowerCase()} ${pathTemplate} ${status}`]
  if (!schema) return value
  const issues: string[] = []
  validate(schema, value, '$', issues)
  if (issues.length) throw new OpenApiParseError(issues)
  return value
}

function validate(
  schema: OpenApiRuntimeSchema,
  value: unknown,
  path: string,
  issues: string[],
): void {
  if (schema === true) return
  if (schema === false) { issues.push(`${path} is forbidden`); return }
  if (schema.$ref) {
    const name = schema.$ref.split('/').at(-1) ?? ''
    const target = openApiSchemas[name]
    if (!target) issues.push(`${path} references unknown schema ${name}`)
    else validate(target, value, path, issues)
    return
  }
  if (schema.allOf) {
    validate(mergeAllOf(schema), value, path, issues)
    return
  }
  if (schema.const !== undefined && !same(value, schema.const)) {
    issues.push(`${path} must equal ${JSON.stringify(schema.const)}`)
  }
  if (schema.enum && !schema.enum.some((item) => same(value, item))) {
    issues.push(`${path} is not an allowed value`)
  }
  validateUnion(schema.oneOf, value, path, issues, true)
  validateUnion(schema.anyOf, value, path, issues, false)
  if (schema.type && !matchesType(schema.type, value)) {
    issues.push(`${path} must be ${Array.isArray(schema.type) ? schema.type.join('|') : schema.type}`)
    return
  }
  if (typeof value === 'string') validateString(schema, value, path, issues)
  if (typeof value === 'number') {
    if (schema.minimum !== undefined && value < schema.minimum) issues.push(`${path} is too small`)
    if (schema.maximum !== undefined && value > schema.maximum) issues.push(`${path} is too large`)
  }
  if (Array.isArray(value) && schema.items) {
    value.forEach((item, index) => validate(schema.items!, item, `${path}[${index}]`, issues))
  }
  if (isRecord(value)) validateObject(schema, value, path, issues)
}

function mergeAllOf(schema: Exclude<OpenApiRuntimeSchema, boolean>): OpenApiRuntimeSchema {
  const { allOf, ...base } = schema
  return (allOf ?? []).reduce<Exclude<OpenApiRuntimeSchema, boolean>>(
    (merged, part) => mergeSchema(merged, resolveSchema(part)),
    base,
  )
}

function resolveSchema(schema: OpenApiRuntimeSchema): Exclude<OpenApiRuntimeSchema, boolean> {
  if (schema === true) return {}
  if (schema === false) return { enum: [] }
  if (schema.$ref) {
    const name = schema.$ref.split('/').at(-1) ?? ''
    return resolveSchema(openApiSchemas[name] ?? false)
  }
  return schema.allOf ? resolveSchema(mergeAllOf(schema)) : schema
}

function mergeSchema(
  left: Exclude<OpenApiRuntimeSchema, boolean>,
  right: Exclude<OpenApiRuntimeSchema, boolean>,
): Exclude<OpenApiRuntimeSchema, boolean> {
  const additionalProperties = left.additionalProperties === false || right.additionalProperties === false
    ? false
    : right.additionalProperties ?? left.additionalProperties
  return {
    ...left,
    ...right,
    properties: { ...left.properties, ...right.properties },
    required: [...new Set([...(left.required ?? []), ...(right.required ?? [])])],
    ...(additionalProperties === undefined ? {} : { additionalProperties }),
  }
}

function validateUnion(
  alternatives: OpenApiRuntimeSchema[] | undefined,
  value: unknown,
  path: string,
  issues: string[],
  exactlyOne: boolean,
): void {
  if (!alternatives) return
  const matches = alternatives.filter((candidate) => {
    const candidateIssues: string[] = []
    validate(candidate, value, path, candidateIssues)
    return candidateIssues.length === 0
  }).length
  if ((exactlyOne && matches !== 1) || (!exactlyOne && matches === 0)) {
    issues.push(`${path} does not match ${exactlyOne ? 'exactly one' : 'any'} allowed shape`)
  }
}

function validateString(
  schema: Exclude<OpenApiRuntimeSchema, boolean>,
  value: string,
  path: string,
  issues: string[],
): void {
  if (schema.minLength !== undefined && [...value].length < schema.minLength) issues.push(`${path} is too short`)
  if (schema.maxLength !== undefined && [...value].length > schema.maxLength) issues.push(`${path} is too long`)
  if (schema.pattern && !new RegExp(schema.pattern, 'u').test(value)) issues.push(`${path} has an invalid format`)
}

function validateObject(
  schema: Exclude<OpenApiRuntimeSchema, boolean>,
  value: Record<string, unknown>,
  path: string,
  issues: string[],
): void {
  for (const key of schema.required ?? []) {
    if (!(key in value)) issues.push(`${path}.${key} is required`)
  }
  for (const [key, item] of Object.entries(value)) {
    const property = schema.properties?.[key]
    if (property) validate(property, item, `${path}.${key}`, issues)
    else if (schema.additionalProperties === false) issues.push(`${path}.${key} is not allowed`)
    else if (schema.additionalProperties && typeof schema.additionalProperties === 'object') {
      validate(schema.additionalProperties, item, `${path}.${key}`, issues)
    }
  }
}

function matchesType(type: string | string[], value: unknown): boolean {
  const candidates = Array.isArray(type) ? type : [type]
  return candidates.some((candidate) => candidate === 'null' ? value === null
    : candidate === 'array' ? Array.isArray(value)
      : candidate === 'object' ? isRecord(value)
        : candidate === 'integer' ? Number.isInteger(value)
          : candidate === 'number' ? typeof value === 'number' && Number.isFinite(value)
            : typeof value === candidate)
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function same(left: unknown, right: unknown): boolean {
  return JSON.stringify(left) === JSON.stringify(right)
}
