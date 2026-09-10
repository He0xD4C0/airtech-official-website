import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import ContentOutline from './ContentOutline.vue'

const sections = [
  { id: 'editor-basics', label: '基本信息' },
  { id: 'editor-composition', label: '页面组成', level: 1 as const },
  { id: 'editor-body', label: '正文', level: 1 as const },
]

function render(activeId: string): Promise<string> {
  return renderToString(createSSRApp(ContentOutline, { sections, activeId }))
}

describe('ContentOutline', () => {
  it('exposes a labelled navigation landmark with the ordered sections', async () => {
    const html = await render('editor-basics')

    expect(html).toContain('aria-label="编辑器大纲"')
    expect(html.indexOf('基本信息')).toBeLessThan(html.indexOf('页面组成'))
    expect(html.indexOf('页面组成')).toBeLessThan(html.indexOf('正文'))
  })

  it('marks exactly the active section with an aria-current state', async () => {
    const html = await render('editor-body')

    expect(html.match(/aria-current="true"/gu)).toHaveLength(1)
    expect(html).toMatch(/<span[^>]*>正文<\/span>/u)
  })
})
