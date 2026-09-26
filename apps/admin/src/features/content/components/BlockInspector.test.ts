import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it, vi } from 'vitest'
import type { ContentBlock, ContentRelationReference } from '@airtek/contracts'

vi.mock('@/features/content/services/contentApi', () => ({
  contentApi: {
    listMediaAssets: vi.fn(async () => ({ items: [], nextCursor: null })),
    getContentRecord: vi.fn(async () => ({ record: { draft: { title: '' } }, etag: null })),
    listContent: vi.fn(async () => ({ items: [], nextCursor: null, total: 0, counts: {} })),
    searchProducts: vi.fn(async () => ({ items: [], nextCursor: null })),
  },
}))

import BlockInspector from '@/features/content/components/BlockInspector.vue'

function render(modelValue: ContentBlock, relations: ContentRelationReference[] = []) {
  return renderToString(createSSRApp(BlockInspector, { modelValue, relations }))
}

describe('BlockInspector', () => {
  it('locks the block type and warns about an empty hero', async () => {
    const html = await render({
      type: 'hero',
      id: '22222222-2222-4222-8222-222222222222',
      eyebrow: null,
      heading: null,
      lead: null,
      media: null,
      actions: [],
      variant: 'standard',
    })

    expect(html).toContain('区块类型（不可切换）')
    expect(html).toContain('Hero 首屏')
    expect(html).toContain('Hero 还没有标题或媒体。')
    expect(html).toContain('媒体只能从资产库选择，不接受手填 UUID 或 URL。')
  })

  it('offers only an asset-library media field, never a raw asset id input', async () => {
    const html = await render({
      type: 'media',
      id: '33333333-3333-4333-8333-333333333333',
      caption: null,
      layout: 'inline',
      media: {
        asset: { assetId: '44444444-4444-4444-8444-444444444444' },
        altText: null,
        decorative: false,
      },
    })

    expect(html).toContain('更换')
    expect(html).toContain('移除媒体')
    expect(html).not.toContain('Asset ID')
    expect(html).not.toContain('placeholder="媒体 URL"')
    expect(html).toContain('图注')
  })

  it('flags a download block without link copy', async () => {
    const html = await render({
      type: 'downloadAsset',
      id: '66666666-6666-4666-8666-666666666666',
      asset: { assetId: '77777777-7777-4777-8777-777777777777' },
      label: '',
      description: null,
    })

    expect(html).toContain('下载区块缺少链接文案。')
    expect(html).toContain('移除下载文件')
  })

  it('explains that an empty relation collection has no linked entities yet', async () => {
    const blockId = '99999999-9999-4999-8999-999999999999'
    const html = await render({
      type: 'relationCollection',
      id: blockId,
      heading: null,
      presentation: 'cards',
      relationIds: [],
    }, [{ id: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', slot: blockId, target: { targetType: 'content', contentId: 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb' } }])

    expect(html).not.toContain('关联集合还没有关联任何实体。')
    expect(html).toContain('受内容记录管理')
    expect(html).toContain('添加关联')
  })

  it('requires confirmation before deleting a block', async () => {
    const html = await render({
      type: 'body',
      id: 'cccccccc-cccc-4ccc-8ccc-cccccccccccc',
      width: 'standard',
    })

    expect(html).toContain('删除正文区块')
    expect(html).not.toContain('确认删除')
  })
})
