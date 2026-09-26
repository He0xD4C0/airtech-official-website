import type { JsonValue } from '@airtek/contracts'

export interface RichTextNode {
  type: string
  text?: string
  attrs?: Record<string, unknown>
  marks?: RichTextMark[]
  content?: RichTextNode[]
}

export interface RichTextMark {
  type: string
  attrs?: Record<string, unknown>
}

const controlCharacters = /[\u0000-\u001f\u007f]/
const internalBase = 'https://public.airtek.invalid'

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

export function asRichTextNode(
  value: JsonValue | unknown,
  depth = 0,
  state: { count: number } = { count: 0 },
): RichTextNode | undefined {
  state.count += 1
  if (depth > 32 || state.count > 2_000) return undefined
  if (!isRecord(value) || typeof value.type !== 'string') return undefined
  return {
    type: value.type,
    text: typeof value.text === 'string' ? value.text : undefined,
    attrs: isRecord(value.attrs) ? value.attrs : undefined,
    marks: Array.isArray(value.marks)
      ? value.marks.flatMap((mark) => {
          if (!isRecord(mark) || typeof mark.type !== 'string') return []
          return [{ type: mark.type, attrs: isRecord(mark.attrs) ? mark.attrs : undefined }]
        })
      : undefined,
    content: Array.isArray(value.content)
      ? value.content.flatMap((child) => {
          const node = asRichTextNode(child, depth + 1, state)
          return node ? [node] : []
        })
      : undefined,
  }
}

function safeInternalUrl(value: string): string | undefined {
  if (!value.startsWith('/') || value.startsWith('//') || value.includes('\\') || controlCharacters.test(value)) return undefined
  try {
    const parsed = new URL(value, internalBase)
    if (parsed.origin !== internalBase || !parsed.pathname.startsWith('/')) return undefined
    return `${parsed.pathname}${parsed.search}${parsed.hash}`
  } catch {
    return undefined
  }
}

function safeHttpsUrl(value: string): string | undefined {
  try {
    const parsed = new URL(value)
    if (parsed.protocol !== 'https:' || parsed.username || parsed.password) return undefined
    return parsed.toString()
  } catch {
    return undefined
  }
}

export function safeLinkUrl(value: unknown): string | undefined {
  if (typeof value !== 'string' || value.length > 2_048 || controlCharacters.test(value)) return undefined
  const trimmed = value.trim()
  if (trimmed.startsWith('/')) return safeInternalUrl(trimmed)
  if (/^#[A-Za-z][A-Za-z0-9_.:-]{0,127}$/.test(trimmed)) return trimmed
  const https = safeHttpsUrl(trimmed)
  if (https) return https
  if (/^mailto:[A-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Z0-9.-]+\.[A-Z]{2,}$/i.test(trimmed)) return trimmed
  if (/^tel:\+?[0-9(). -]{5,32}$/.test(trimmed)) return trimmed
  return undefined
}

export function safeImageUrl(value: unknown): string | undefined {
  if (typeof value !== 'string' || value.length > 2_048 || controlCharacters.test(value)) return undefined
  const trimmed = value.trim()
  return trimmed.startsWith('/') ? safeInternalUrl(trimmed) : safeHttpsUrl(trimmed)
}

export function richTextPlainText(node: RichTextNode | undefined): string {
  if (!node) return ''
  if (node.type === 'text') return node.text ?? ''
  return (node.content ?? []).map(richTextPlainText).join(node.type === 'paragraph' ? ' ' : '')
}
