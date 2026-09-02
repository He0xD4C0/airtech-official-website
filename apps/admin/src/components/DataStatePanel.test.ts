import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import DataStatePanel from './DataStatePanel.vue'

describe('DataStatePanel', () => {
  it.each([
    ['loading', '正在读取数据库'],
    ['empty', '暂无记录'],
    ['error', '数据读取失败'],
    ['forbidden', '没有访问权限'],
  ] as const)('renders the %s state explicitly', async (state, message) => {
    const html = await renderToString(createSSRApp(DataStatePanel, { state }))
    expect(html).toContain(message)
  })

  it('only exposes retry for an error and a configured action for empty data', async () => {
    const errorHtml = await renderToString(createSSRApp(DataStatePanel, { state: 'error' }))
    const emptyHtml = await renderToString(createSSRApp(DataStatePanel, {
      state: 'empty',
      actionLabel: '开始配置',
    }))
    expect(errorHtml).toContain('重新加载')
    expect(emptyHtml).toContain('开始配置')
    expect(emptyHtml).not.toContain('重新加载')
  })
})
