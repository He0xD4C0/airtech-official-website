import { describe, expect, it } from 'vitest'
import { extractArticleOutline } from './articlePresentation'
import { tiptapDocument } from '@/test/publicProjectionFixture'

describe('native V2 article presentation data', () => {
  it('creates stable unique heading anchors from the Tiptap document', () => {
    const body = tiptapDocument([
      { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'Read the data' }] },
      { type: 'paragraph', content: [{ type: 'text', text: 'Body.' }] },
      { type: 'heading', attrs: { level: 3 }, content: [{ type: 'text', text: 'Read the data' }] },
    ])

    expect(extractArticleOutline(body)).toEqual([
      { id: 'read-the-data', title: 'Read the data', level: 2 },
      { id: 'read-the-data-2', title: 'Read the data', level: 3 },
    ])
  })

  it('omits malformed or empty headings without inventing outline labels', () => {
    const body = tiptapDocument([
      { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'Unsafe\u0000heading' }] },
      { type: 'paragraph', content: [{ type: 'text', text: 'Body only.' }] },
    ])
    expect(extractArticleOutline(body)).toEqual([])
    expect(extractArticleOutline(null)).toEqual([])
  })
})
