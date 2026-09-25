import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it, vi } from 'vitest'
import type { CmsContentKind, ContentTypeFields } from '@airtek/contracts'

vi.mock('@/services/contentApi', () => ({
  contentApi: {
    listMediaAssets: vi.fn(async () => ({ items: [], nextCursor: null })),
    getContentRecord: vi.fn(async () => ({ record: { draft: { title: '' } }, etag: null })),
    listContent: vi.fn(async () => ({ items: [], nextCursor: null, total: 0, counts: {} })),
    searchProducts: vi.fn(async () => ({ items: [], nextCursor: null })),
  },
}))

import TypeFieldsPanel from './TypeFieldsPanel.vue'

function render(modelValue: ContentTypeFields, kind?: CmsContentKind) {
  return renderToString(createSSRApp(TypeFieldsPanel, { modelValue, kind }))
}

describe('TypeFieldsPanel', () => {
  it('renders the FAQ list editor for FAQ content instead of a free-form document editor', async () => {
    const html = await render({ type: 'faq', items: [] })
    expect(html).toContain('FAQ 条目（0）')
    expect(html).toContain('添加条目')
  })

  it('renders site-level editors for general information, navigation and footer', async () => {
    const general = await render({
      type: 'generalInformation',
      contact: { addressLines: [], email: null, phone: null },
      defaultSeo: { indexable: false, title: null, description: null, socialImage: null },
      socialLinks: [],
      productCategories: [],
      navigationCta: null,
      organizationName: null,
    })
    expect(general).toContain('站点级配置不参与页面组成')
    expect(general).toContain('联系方式')
    expect(general).toContain('站点图标')
    expect(general).toContain('发布就绪警告')
    expect(general).toContain('至少 512 × 512')
    expect(general).toContain('中性占位图标')

    const navigation = await render({ type: 'navigation', items: [] })
    expect(navigation).toContain('主导航保存在站点配置中')

    const footer = await render({ type: 'footer', columns: [], legalLinks: [] })
    expect(footer).toContain('页脚栏目')
    expect(footer).toContain('法律链接')
  })

  it('states that the content type is locked and never renders a type switcher', async () => {
    const html = await render({ type: 'page' }, 'page')
    expect(html).toContain('内容类型锁定为 页面')
    expect(html).toContain('编辑器不提供切换入口')
    expect(html).not.toContain('type="radio"')
  })

  it('fails closed when the draft kind and the type fields disagree', async () => {
    const html = await render({ type: 'page' }, 'news')

    expect(html).toContain('role="alert"')
    expect(html).toContain('与模板锁定的内容类型')
    expect(html).toContain('已停止结构编辑')
    expect(html).not.toContain('该内容类型没有额外的结构化字段')
  })
})
