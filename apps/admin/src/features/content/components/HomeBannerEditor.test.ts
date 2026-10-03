// @vitest-environment jsdom
import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import type { ContentBlock } from '@airtek/contracts'
import HomeBannerEditor from './HomeBannerEditor.vue'

const first: ContentBlock = { type: 'hero', id: 'one', heading: 'Original', eyebrow: null, lead: null, media: null, actions: [], variant: 'standard' }
const body: ContentBlock = { type: 'body', id: 'body', width: 'standard' }

describe('homepage banner management', () => {
  it('keeps one banner, adds and reorders pages while preserving other content', async () => {
    const wrapper = mount(HomeBannerEditor, {
      props: { modelValue: [first, body], title: 'Home' },
      global: { stubs: { BlockFieldsEditor: true } },
    })
    const apply = async () => {
      const events = wrapper.emitted('update:modelValue')!
      const next = events[events.length - 1][0] as ContentBlock[]
      await wrapper.setProps({ modelValue: next })
      return next
    }
    expect(wrapper.find('[aria-label="删除 Banner 1"]').attributes('disabled')).toBeDefined()
    await wrapper.findAll('button').find(button => button.text() === '添加 Banner 页面')!.trigger('click')
    const added = await apply()
    expect(added.map(block => block.type)).toEqual(['hero', 'hero', 'body'])
    const secondId = added[1].id
    await wrapper.find('[aria-label="上移 Banner 2"]').trigger('click')
    expect((await apply()).map(block => block.id)).toEqual([secondId, 'one', 'body'])
    await wrapper.find('[aria-label="删除 Banner 2"]').trigger('click')
    expect((await apply()).map(block => block.id)).toEqual([secondId, 'body'])
    expect(wrapper.find('[aria-label="删除 Banner 1"]').attributes('disabled')).toBeDefined()
    wrapper.unmount()
  })
})
