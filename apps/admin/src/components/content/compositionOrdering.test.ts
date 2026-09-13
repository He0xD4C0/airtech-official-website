import { describe, expect, it } from 'vitest'
import type { ContentBlock, ContentBlockKind } from '@airtek/contracts'
import { addableBlockKinds, canMoveBlock, moveBlock, requiredBlockSet } from './compositionOrdering'

const requiredBlocks: ContentBlockKind[] = ['hero', 'body']

function block(type: ContentBlockKind, id: string): ContentBlock {
  switch (type) {
    case 'hero':
      return { type, id, eyebrow: null, heading: null, lead: null, media: null, actions: [], variant: 'standard' }
    case 'body':
      return { type, id, width: 'standard' }
    case 'media':
      return {
        type,
        id,
        media: { asset: { assetId: 'asset-1' }, altText: null, decorative: true },
        caption: null,
        layout: 'inline',
      }
    case 'cta':
      return {
        type,
        id,
        eyebrow: null,
        heading: 'CTA',
        body: null,
        action: { label: 'Go', target: { targetType: 'route', path: '/' } },
        variant: 'standard',
      }
    default:
      throw new Error(`Unsupported test block: ${type}`)
  }
}

describe('composition ordering', () => {
  it('never adds or moves a required template region', () => {
    const required = requiredBlockSet(requiredBlocks)
    expect(required.has('hero')).toBe(true)

    const blocks = [block('hero', 'hero-1'), block('media', 'media-1')]
    expect(canMoveBlock(blocks, requiredBlocks, 'hero-1', 1)).toBe(false)
    expect(canMoveBlock(blocks, requiredBlocks, 'media-1', -1)).toBe(false)
    expect(moveBlock(blocks, requiredBlocks, 'hero-1', 1)).toBeNull()
  })

  it('swaps two adjacent optional blocks and keeps the rest untouched', () => {
    const blocks = [block('hero', 'hero-1'), block('media', 'media-1'), block('cta', 'cta-1')]

    expect(canMoveBlock(blocks, requiredBlocks, 'media-1', 1)).toBe(true)
    expect(moveBlock(blocks, requiredBlocks, 'media-1', 1)?.map((entry) => entry.id))
      .toEqual(['hero-1', 'cta-1', 'media-1'])
    expect(blocks.map((entry) => entry.id)).toEqual(['hero-1', 'media-1', 'cta-1'])
  })

  it('lists required kinds as unavailable for adding', () => {
    expect(addableBlockKinds(['hero', 'body', 'media', 'cta'], requiredBlocks))
      .toEqual(['media', 'cta'])
  })
})
