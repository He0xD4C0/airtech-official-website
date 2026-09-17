import { beforeAll, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import MediaLightbox from './MediaLightbox.vue'

beforeAll(() => {
  HTMLDialogElement.prototype.showModal = vi.fn(function (this: HTMLDialogElement) {
    this.setAttribute('open', '')
  })
  HTMLDialogElement.prototype.close = vi.fn(function (this: HTMLDialogElement) {
    this.removeAttribute('open')
    this.dispatchEvent(new Event('close'))
  })
})

describe('MediaLightbox', () => {
  it('does not request the original source until the user opens the dialog', async () => {
    const wrapper = mount(MediaLightbox, {
      props: {
        previewSrc: 'https://media.example/preview.webp',
        originalSrc: 'https://media.example/original.png',
        alt: 'Fan assembly',
      },
    })
    expect(wrapper.html()).toContain('https://media.example/preview.webp')
    expect(wrapper.html()).not.toContain('https://media.example/original.png')
    await wrapper.get('button').trigger('click')
    expect(wrapper.html()).toContain('https://media.example/original.png')
    expect(wrapper.get('dialog').attributes('open')).toBeDefined()
  })
})
