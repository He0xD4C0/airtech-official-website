import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import RevisionDiff from './RevisionDiff.vue'

describe('RevisionDiff', () => {
  it('renders every changed path with before and after values', async () => {
    const html = await renderToString(createSSRApp(RevisionDiff, {
      changes: [
        { path: '$.title', before: 'Old title', after: 'New title' },
        { path: '$.composition.blocks', before: [], after: [{ type: 'hero' }] },
      ],
    }))

    expect(html).toContain('2 处差异')
    expect(html).toContain('$.title')
    expect(html).toContain('Old title')
    expect(html).toContain('New title')
    expect(html).toContain('按字段路径筛选差异')
  })

  it('renders the loading state before results arrive', async () => {
    const html = await renderToString(createSSRApp(RevisionDiff, { changes: [], loading: true }))
    expect(html).toContain('正在计算差异')
    expect(html).not.toContain('处差异')
  })

  it('renders a server diff failure as an alert instead of an empty diff', async () => {
    const html = await renderToString(createSSRApp(RevisionDiff, {
      changes: [],
      error: '无法读取修订差异。',
    }))

    expect(html).toContain('role="alert"')
    expect(html).toContain('无法读取修订差异。')
  })

  it('renders the configured empty message when there is nothing to compare', async () => {
    const html = await renderToString(createSSRApp(RevisionDiff, {
      changes: [],
      emptyMessage: '所选修订与目标之间没有差异。',
    }))

    expect(html).toContain('所选修订与目标之间没有差异。')
  })
})
