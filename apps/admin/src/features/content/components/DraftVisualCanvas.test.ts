import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import DraftVisualCanvas from '@airtek/content-renderer/DraftVisualCanvas.vue'
import type { ContentTemplateDefinition } from '@airtek/contracts'
import { draftFromTemplate, newDraftId } from '@/features/content/services/contentDraftDefaults'

const template: ContentTemplateDefinition = {
  key: 'articleDetail',
  contentKind: 'article',
  bodyPolicy: 'required',
  requiredBlocks: ['hero', 'body'],
  allowedBlocks: ['hero', 'body', 'media'],
  routable: true,
  routePattern: '/{locale}/resources/articles/{slug}',
  singletonPerLocale: false,
}

describe('DraftVisualCanvas', () => {
  it('renders unsaved text, links, blocks, and a pending object URL', async () => {
    const document = draftFromTemplate({ template, title: 'Local draft', slug: 'local-draft' })
    document.summary = 'Only stored in browser memory.'
    document.body = {
      type: 'doc',
      content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Unsaved body copy' }] }],
    }
    const hero = document.composition.blocks.find((block) => block.type === 'hero')
    if (!hero || hero.type !== 'hero') throw new Error('Expected the article hero block.')
    hero.actions = [{ label: 'Contact', target: { targetType: 'route', path: '/en/contact' } }]
    document.composition.blocks.push({
      type: 'media',
      id: newDraftId(),
      media: { asset: { assetId: 'pending-image' }, altText: 'Pending fan image', decorative: false },
      caption: 'Pending image caption',
      layout: 'inline',
    })

    const html = await renderToString(createSSRApp(DraftVisualCanvas, {
      document,
      pendingMediaUrls: { 'pending-image': 'blob:https://local.test/pending-image' },
    }))

    expect(html).toContain('Local draft')
    expect(html).toContain('Only stored in browser memory.')
    expect(html).toContain('Unsaved body copy')
    expect(html).toContain('href="/en/contact"')
    expect(html).toContain('src="blob:https://local.test/pending-image"')
    expect(html).toContain('alt="Pending fan image"')
  })
})
