import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import type { ContentRevisionV2 } from '@airtek/contracts'
import RevisionTimeline from './RevisionTimeline.vue'

function revision(overrides: Partial<ContentRevisionV2>): ContentRevisionV2 {
  return {
    contentId: '11111111-1111-4111-8111-111111111111',
    createdAt: '2026-09-10T04:00:00Z',
    createdBy: 'ops@airtek.test',
    document: {} as ContentRevisionV2['document'],
    kind: 'manual',
    reason: 'Create a manual snapshot from the Admin CMS editor',
    revision: 7,
    sourceDraftVersion: 3,
    ...overrides,
  }
}

describe('RevisionTimeline', () => {
  it('lists revisions newest first and marks the published revision', async () => {
    const html = await renderToString(createSSRApp(RevisionTimeline, {
      revisions: [
        revision({ revision: 7, kind: 'manual' }),
        revision({ revision: 8, kind: 'publish', reason: 'Publish the current Admin CMS draft' }),
      ],
      currentDraftVersion: 9,
      publishedRevision: 8,
    }))

    expect(html.indexOf('r8')).toBeLessThan(html.indexOf('r7'))
    expect(html).toContain('已发布')
    expect(html).toContain('当前草稿 v9')
    expect(html).toContain('发布')
    expect(html).toContain('对比设置')
  })

  it('explains the empty state instead of rendering an empty list', async () => {
    const html = await renderToString(createSSRApp(RevisionTimeline, {
      revisions: [],
      currentDraftVersion: 1,
      publishedRevision: null,
    }))

    expect(html).toContain('还没有修订')
    expect(html).toContain('尚未发布')
    expect(html).not.toContain('对比设置')
  })

  it('keeps the destructive restore form closed until an editor opens it', async () => {
    const html = await renderToString(createSSRApp(RevisionTimeline, {
      revisions: [revision({ revision: 2 })],
      currentDraftVersion: 3,
      publishedRevision: null,
    }))

    expect(html).not.toContain('确认恢复')
    expect(html).toContain('恢复')
  })
})
