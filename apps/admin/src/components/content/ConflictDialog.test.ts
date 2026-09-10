import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import type { ContentDraftV2 } from '@airtek/contracts'

import ConflictDialog from './ConflictDialog.vue'

function draft(overrides: Partial<ContentDraftV2>): ContentDraftV2 {
  return {
    schemaVersion: 2,
    kind: 'news',
    locale: 'en',
    templateKey: 'newsDetail',
    draftVersion: 4,
    title: 'IE3 motor launch',
    slug: 'ie3-motor-launch',
    summary: null,
    isPlaceholder: false,
    body: null,
    composition: { blocks: [] },
    relations: [],
    seo: { indexable: true, title: null, description: null, socialImage: null },
    typeFields: {
      type: 'news',
      category: null,
      authorDisplayName: null,
      publicationAt: null,
      cover: null,
      featured: false,
    },
    ...overrides,
  }
}

async function renderDialog(props: Record<string, unknown>): Promise<string> {
  const context: Record<string, unknown> = {}
  await renderToString(createSSRApp(ConflictDialog, props), context)
  const teleports = (context.teleports ?? {}) as Record<string, string>
  return teleports.body ?? teleports['#body'] ?? ''
}

describe('ConflictDialog', () => {
  it('stays closed until a conflict is detected', async () => {
    const html = await renderDialog({
      open: false,
      localDraft: draft({}),
      serverDraft: draft({ draftVersion: 5 }),
      changes: [],
    })

    expect(html).not.toContain('保存已停止')
    expect(html).not.toContain('role="dialog"')
  })

  it('shows both draft versions, the diff and the two supported recovery actions', async () => {
    const html = await renderDialog({
      open: true,
      localDraft: draft({ title: 'Local draft', draftVersion: 4 }),
      serverDraft: draft({ title: 'Server draft', draftVersion: 6 }),
      changes: [{ path: '$.title', before: 'Server draft', after: 'Local draft' }],
    })

    expect(html).toContain('保存已停止：检测到并发编辑')
    expect(html).toContain('v4 · Local draft')
    expect(html).toContain('v6 · Server draft')
    expect(html).toContain('$.title')
    expect(html).toContain('下载本地 JSON')
    expect(html).toContain('重新载入服务器版本')
    expect(html).toContain('role="dialog"')
    expect(html).toContain('aria-modal="true"')
  })

  it('warns that the server version could not be read instead of pretending the diff is complete', async () => {
    const html = await renderDialog({
      open: true,
      localDraft: draft({}),
      serverDraft: null,
      changes: [],
    })

    expect(html).toContain('无法读取服务器版本')
    expect(html).toContain('无法读取')
  })
})
