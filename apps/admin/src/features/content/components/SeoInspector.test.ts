import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it, vi } from 'vitest'
import type { ContentTemplateDefinition, SeoInputV2 } from '@airtek/contracts'

vi.mock('@/features/content/services/contentApi', () => ({
  contentApi: {
    listMediaAssets: vi.fn(async () => ({ items: [], nextCursor: null })),
    getContentRecord: vi.fn(async () => ({ record: { draft: { title: '' } }, etag: null })),
    listContent: vi.fn(async () => ({ items: [], nextCursor: null, total: 0, counts: {} })),
    searchProducts: vi.fn(async () => ({ items: [], nextCursor: null })),
  },
}))

import SeoInspector from '@/features/content/components/SeoInspector.vue'

const seo: SeoInputV2 = { title: 'IE3 motors', description: 'Efficient motors', indexable: true, socialImage: null }

const detailed: ContentTemplateDefinition = {
  key: 'newsDetail',
  contentKind: 'news',
  routable: true,
  routePattern: '/{locale}/resources/news/{slug}',
  singletonPerLocale: false,
  requiredBlocks: [],
  allowedBlocks: ['body'],
  bodyPolicy: 'optional',
}

const singleton: ContentTemplateDefinition = {
  key: 'generalInformation',
  contentKind: 'generalInformation',
  routable: false,
  routePattern: null,
  singletonPerLocale: true,
  requiredBlocks: [],
  allowedBlocks: [],
  bodyPolicy: 'forbidden',
}

describe('SeoInspector', () => {
  it('derives the canonical path from the template registry pattern instead of accepting input', async () => {
    const html = await renderToString(createSSRApp(SeoInspector, {
      modelValue: seo,
      slug: 'ie3-motors',
      locale: 'en',
      template: detailed,
      isPlaceholder: false,
    }))

    expect(html).toContain('/en/resources/news/ie3-motors')
    expect(html).toContain('不接受手填')
    expect(html).toContain('搜索结果预览')
  })

  it('forces noindex for placeholder content and explains why the toggle is disabled', async () => {
    const html = await renderToString(createSSRApp(SeoInspector, {
      modelValue: seo,
      slug: 'ie3-motors',
      locale: 'en',
      template: detailed,
      isPlaceholder: true,
    }))

    expect(html).toContain('占位内容强制 noindex')
    expect(html).toContain('disabled')
    expect(html).toContain('noindex：该页面不会出现在搜索引擎结果中。')
  })

  it('reports an invalid slug without silently rewriting it', async () => {
    const html = await renderToString(createSSRApp(SeoInspector, {
      modelValue: seo,
      slug: 'IE3 Motors/',
      locale: 'en',
      template: detailed,
      isPlaceholder: false,
    }))

    expect(html).toContain('slug 只允许小写字母、数字与连字符')
    expect(html).toContain('aria-invalid="true"')
  })

  it('hides slug and canonical inputs for a non-routable singleton template', async () => {
    const html = await renderToString(createSSRApp(SeoInspector, {
      modelValue: { ...seo, indexable: false },
      slug: null,
      locale: 'en',
      template: singleton,
      isPlaceholder: false,
    }))

    expect(html).toContain('没有公开路由')
    expect(html).not.toContain('Slug')
    expect(html).toContain('该内容没有公开 URL（无 canonical）')
  })

  it('does not request a slug when the registry pattern has no slug placeholder', async () => {
    const html = await renderToString(createSSRApp(SeoInspector, {
      modelValue: seo,
      slug: null,
      locale: 'en',
      template: { ...detailed, key: 'about', contentKind: 'page', routePattern: '/{locale}/company/about' },
      isPlaceholder: false,
    }))

    expect(html).toContain('/en/company/about')
    expect(html).not.toContain('Slug<small>')
  })
})
