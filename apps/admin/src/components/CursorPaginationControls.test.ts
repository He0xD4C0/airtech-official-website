import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import CursorPaginationControls from './CursorPaginationControls.vue'

describe('CursorPaginationControls', () => {
  it('renders the caller-provided Chinese count unit once', async () => {
    const html = await renderToString(createSSRApp(CursorPaginationControls, {
      itemCount: 18,
      pageNumber: 1,
      canPrevious: false,
      canNext: false,
      label: '条记录，共 18 条',
    }))

    expect(html).toContain('第 1 页 · 当前 18 条记录，共 18 条')
    expect(html).not.toContain('条条记录')
  })
})
