import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import type { ContentBlock, ContentTemplateDefinition } from '@airtek/contracts'
import CompositionEditor from './CompositionEditor.vue'

const template: ContentTemplateDefinition = {
  key: 'articleDetail',
  contentKind: 'article',
  bodyPolicy: 'required',
  requiredBlocks: ['hero', 'body'],
  allowedBlocks: ['hero', 'body', 'media', 'cta'],
  routable: true,
  routePattern: '/{locale}/resources/articles/{slug}',
  singletonPerLocale: false,
}

function hero(id: string): ContentBlock {
  return {
    type: 'hero',
    id,
    eyebrow: null,
    heading: 'Hero heading',
    lead: null,
    media: null,
    actions: [],
    variant: 'standard',
  }
}

function media(id: string): ContentBlock {
  return {
    type: 'media',
    id,
    media: { asset: { assetId: 'asset-1', versionId: 'version-1' }, altText: 'Product photo', decorative: false },
    caption: null,
    layout: 'inline',
  }
}

function render(blocks: ContentBlock[], definition: ContentTemplateDefinition = template): Promise<string> {
  return renderToString(createSSRApp(CompositionEditor, {
    modelValue: blocks,
    template: definition,
    selectedBlockId: null,
  }))
}

describe('CompositionEditor', () => {
  it('marks required template regions and disables their move and remove controls', async () => {
    const html = await render([hero('hero-1'), media('media-1')])

    expect(html).toContain('>必需<')
    expect(html).toMatch(/aria-label="移除Hero 首屏区块"[^>]*disabled/u)
    expect(html).toMatch(/aria-label="上移Hero 首屏区块"[^>]*disabled/u)
    expect(html).not.toMatch(/aria-label="移除媒体区块"[^>]*disabled/u)
  })

  it('offers only optional allowed kinds in the add menu', async () => {
    const html = await render([hero('hero-1')])

    expect(html).toContain('aria-label="添加媒体区块"')
    expect(html).toContain('aria-label="添加CTA区块"')
    expect(html).not.toContain('aria-label="添加Hero 首屏区块"')
    expect(html).not.toContain('aria-label="添加正文区块"')
  })

  it('explains when a controlled template does not use composition blocks', async () => {
    const html = await render([], {
      key: 'navigation',
      contentKind: 'navigation',
      bodyPolicy: 'forbidden',
      requiredBlocks: [],
      allowedBlocks: [],
      routable: false,
      routePattern: null,
      singletonPerLocale: true,
    })

    expect(html).toContain('该模板不使用页面组成区块')
  })
})
