import type { TiptapDocument, TiptapNode } from '@airtek/contracts'

/**
 * Admin-side plain-text bridge for small rich-text fields (FAQ answers).
 *
 * The canonical rich-text editor owns full formatting; this helper only exists
 * so compact list fields can be edited without inventing a second rich-text
 * runtime. Rebuilding a document from plain text intentionally replaces marks.
 */
export function documentToPlainText(document: TiptapDocument | null | undefined): string {
  if (!document?.content) return ''
  return document.content.map(textOfNode).join('').replace(/\n+$/u, '')
}

export function plainTextToDocument(value: string): TiptapDocument {
  const content: TiptapNode[] = value.split('\n').map((line) => ({
    type: 'paragraph' as const,
    content: line ? [{ type: 'text' as const, text: line }] : [],
  }))
  return { type: 'doc', content }
}

function textOfNode(node: TiptapNode): string {
  const record = node as Record<string, unknown>
  if (typeof record.text === 'string') return record.text
  const children = Array.isArray(record.content) ? (record.content as TiptapNode[]) : []
  const inner = children.map(textOfNode).join('')
  return record.type === 'paragraph' || record.type === 'heading' ? `${inner}\n` : inner
}
