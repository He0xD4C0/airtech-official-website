export interface JsonRevisionDifference {
  path: string
  before: unknown
  after: unknown
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function equalJson(left: unknown, right: unknown): boolean {
  return JSON.stringify(left) === JSON.stringify(right)
}

export function diffJsonRevisions(
  before: unknown,
  after: unknown,
  path = '$',
): JsonRevisionDifference[] {
  if (equalJson(before, after)) return []
  if (!isRecord(before) || !isRecord(after)) return [{ path, before, after }]

  const differences: JsonRevisionDifference[] = []
  const keys = new Set([...Object.keys(before), ...Object.keys(after)])
  for (const key of [...keys].sort()) {
    differences.push(...diffJsonRevisions(before[key], after[key], `${path}.${key}`))
  }
  return differences
}

