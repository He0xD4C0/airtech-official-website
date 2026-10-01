import { createHash } from 'node:crypto'
import { mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'

const [auditDirectory = '.local/feishu-audit', output = '.local/product-archive/archive.json'] =
  process.argv.slice(2)

const productSources = [
  {
    file: 'centrifugal-snapshot.json',
    name: '离心风机系列 Centrifugal Fans',
    family: 'centrifugal',
    application: null,
  },
  {
    file: 'axial-snapshot.json',
    name: '轴流风机系列 Axial Fans',
    family: 'axial',
    application: null,
  },
  {
    file: 'agriculture-snapshot.json',
    name: '农畜牧业风机 Agriculture & Livestock Fans.',
    family: 'axial',
    application: 'agriculture-livestock',
  },
  {
    file: 'crossflow-snapshot.json',
    name: '贯流风机系列 Cross flow fans',
    family: 'crossFlow',
    application: null,
  },
]

const metadataTables = {
  tblhKpZRLfUIlGYo: 'supplier',
  tblk59WgYKQ48XHz: 'brand',
}

const inputFiles = [
  ...productSources.map((source) => source.file),
  'additional-inventory.json',
  'objects.jsonl',
  'additional-objects.jsonl',
]
const capturedAt = process.env.AIRTEK_ARCHIVE_CAPTURED_AT
  ?? new Date(Math.max(...inputFiles.map((file) => statSync(resolve(auditDirectory, file)).mtimeMs)))
    .toISOString()

const readJson = (path) => JSON.parse(readFileSync(path, 'utf8'))
const jsonLines = (path) => readFileSync(path, 'utf8')
  .trim()
  .split('\n')
  .filter(Boolean)
  .map((line) => JSON.parse(line))

const objectsByToken = new Map()
for (const object of [
  ...jsonLines(resolve(auditDirectory, 'objects.jsonl')),
  ...jsonLines(resolve(auditDirectory, 'additional-objects.jsonl')),
]) {
  if (object.status !== 'stored') continue
  const existing = objectsByToken.get(object.file_token)
  const normalized = {
    fileToken: object.file_token,
    sha256: String(object.sha256).toLowerCase(),
    byteSize: object.actual_size,
    objectKey: object.object_key,
  }
  if (existing && (existing.sha256 !== normalized.sha256 || existing.byteSize !== normalized.byteSize)) {
    throw new Error(`Object token ${object.file_token} has conflicting stored metadata`)
  }
  if (!/^[0-9a-f]{64}$/u.test(normalized.sha256) || !Number.isInteger(normalized.byteSize)) {
    throw new Error(`Object token ${object.file_token} has invalid checksum or size metadata`)
  }
  objectsByToken.set(object.file_token, normalized)
}

const sources = []
const referencedTokens = new Set()
for (const definition of productSources) {
  const snapshot = readJson(resolve(auditDirectory, definition.file))
  const attachmentFields = snapshot.fields.filter((field) => field.type === 17)
  for (const record of snapshot.records) {
    for (const field of attachmentFields) {
      for (const attachment of record.fields[field.field_name] ?? []) {
        if (attachment.file_token) referencedTokens.add(attachment.file_token)
      }
    }
  }
  sources.push({
    source: {
      enabled: true,
      wikiToken: snapshot.wiki,
      tableId: snapshot.table,
      name: definition.name,
      family: definition.family,
      application: definition.application,
    },
    fields: snapshot.fields,
    records: snapshot.records,
  })
}

const missingObjects = [...referencedTokens].filter((token) => !objectsByToken.has(token))
if (missingObjects.length) {
  throw new Error(`Product attachments lack stored archive objects: ${missingObjects.join(', ')}`)
}

const additional = readJson(resolve(auditDirectory, 'additional-inventory.json'))
const metadataSources = additional
  .filter((table) => metadataTables[table.table])
  .map((table) => ({
    kind: metadataTables[table.table],
    name: table.database_title,
    wikiToken: table.wiki,
    tableId: table.table,
    fields: table.fields,
    records: table.records,
  }))

const archive = {
  schemaVersion: 1,
  capturedAt,
  sources,
  metadataSources,
  objects: [...referencedTokens]
    .sort()
    .map((token) => objectsByToken.get(token)),
}
const serialized = `${JSON.stringify(archive, null, 2)}\n`
const archiveSha256 = createHash('sha256').update(serialized).digest('hex')
mkdirSync(dirname(output), { recursive: true })
writeFileSync(output, serialized)

console.log(JSON.stringify({
  output: resolve(output),
  archiveSha256,
  productRecords: sources.reduce((sum, source) => sum + source.records.length, 0),
  metadataRecords: metadataSources.reduce((sum, source) => sum + source.records.length, 0),
  objectCount: archive.objects.length,
  objectBytes: archive.objects.reduce((sum, object) => sum + object.byteSize, 0),
}, null, 2))
