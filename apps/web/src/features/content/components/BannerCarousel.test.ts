import { mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import BannerCarousel, { type BannerSlide } from '@airtek/content-renderer/BannerCarousel.vue'

const slides: BannerSlide[] = [1, 2, 3].map(index => ({
  id: String(index), eyebrow: 'AIRTEKPOWER', heading: `Banner ${index}`, lead: 'Controlled text',
  image: `/test-${index}.png`, alt: `Image ${index}`, actions: [{ label: 'Products', href: '/en/products' }],
}))
const wrappers: Array<ReturnType<typeof mount>> = []
function render(options = {}) {
  const wrapper = mount(BannerCarousel, { props: { slides, ...options } })
  wrappers.push(wrapper)
  return wrapper
}
function active(wrapper: ReturnType<typeof mount>) {
  return wrapper.find('.banner-carousel__slide:not([aria-hidden])').text()
}
beforeEach(() => {
  vi.useFakeTimers()
  vi.spyOn(document, 'hidden', 'get').mockReturnValue(false)
  vi.stubGlobal('matchMedia', vi.fn(() => ({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() })))
})
afterEach(() => {
  wrappers.splice(0).forEach(wrapper => wrapper.unmount())
  vi.useRealTimers()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

describe('three-layer homepage banners', () => {
  it('uses text-free dots and consistent SVG controls', () => {
    const wrapper = render()
    expect(wrapper.findAll('.banner-carousel__dot')).toHaveLength(3)
    expect(wrapper.find('.banner-carousel__dots').text()).toBe('')
    expect(wrapper.findAll('button svg')).toHaveLength(3)
    expect(wrapper.find('.banner-carousel__arrow--previous').attributes('aria-label')).toBe('Previous banner')
  })

  it('moves only current and target slides and wraps in the requested direction', async () => {
    const wrapper = render({ preview: true })
    await wrapper.trigger('keydown', { key: 'ArrowLeft' })
    expect(active(wrapper)).toContain('Banner 3')
    expect(wrapper.attributes('style')).toContain('--banner-direction: -1')
    await vi.advanceTimersByTimeAsync(50)
    expect(wrapper.classes()).toContain('is-moving')
    expect(wrapper.findAll('.is-outgoing')).toHaveLength(1)
    expect(wrapper.findAll('.is-active')).toHaveLength(1)
    await vi.advanceTimersByTimeAsync(500)
    expect(wrapper.findAll('.is-outgoing')).toHaveLength(0)
    await wrapper.trigger('keydown', { key: 'ArrowRight' })
    expect(active(wrapper)).toContain('Banner 1')
    expect(wrapper.attributes('style')).toContain('--banner-direction: 1')
    await vi.advanceTimersByTimeAsync(500)
    await wrapper.find('[aria-label="显示 Banner 3"]').trigger('click')
    expect(active(wrapper)).toContain('Banner 3')
    expect(wrapper.find('.is-outgoing').text()).toContain('Banner 1')
  })

  it('keeps the last navigation target while moving and resets when slides change', async () => {
    const wrapper = render({ preview: true })
    const next = wrapper.find('.banner-carousel__arrow--next')
    await next.trigger('click')
    await next.trigger('click')
    await next.trigger('click')
    expect(active(wrapper)).toContain('Banner 2')
    await vi.advanceTimersByTimeAsync(1000)
    expect(active(wrapper)).toContain('Banner 1')
    expect(wrapper.findAll('.is-outgoing')).toHaveLength(0)
    await next.trigger('click')
    await wrapper.setProps({ slides: [slides[0]] })
    await vi.advanceTimersByTimeAsync(1000)
    expect(active(wrapper)).toContain('Banner 1')
    expect(wrapper.findAll('.is-outgoing')).toHaveLength(0)
  })

  it('keeps manual switching immediate after opting into reduced-motion autoplay', async () => {
    vi.stubGlobal('matchMedia', vi.fn(() => ({ matches: true, addEventListener: vi.fn(), removeEventListener: vi.fn() })))
    const wrapper = render({ slides: slides.slice(0, 2) })
    await wrapper.vm.$nextTick()
    await wrapper.find('[aria-label="Play banners"]').trigger('click')
    await vi.advanceTimersByTimeAsync(6000)
    expect(active(wrapper)).toContain('Banner 2')
    expect(wrapper.classes()).not.toContain('is-preparing')
    await wrapper.find('[aria-label="Next banner"]').trigger('click')
    expect(active(wrapper)).toContain('Banner 1')
    expect(wrapper.findAll('.is-outgoing')).toHaveLength(0)
  })

  it('cycles every six seconds and pauses for hover, focus and hidden pages', async () => {
    const wrapper = render()
    await vi.advanceTimersByTimeAsync(6000)
    expect(active(wrapper)).toContain('Banner 2')
    await wrapper.trigger('mouseenter')
    await vi.advanceTimersByTimeAsync(12000)
    expect(active(wrapper)).toContain('Banner 2')
    await wrapper.trigger('mouseleave')
    await wrapper.trigger('focusin')
    await vi.advanceTimersByTimeAsync(6000)
    expect(active(wrapper)).toContain('Banner 2')
    await wrapper.trigger('focusout', { relatedTarget: null })
    await vi.advanceTimersByTimeAsync(6000)
    expect(active(wrapper)).toContain('Banner 3')
    vi.spyOn(document, 'hidden', 'get').mockReturnValue(true)
    document.dispatchEvent(new Event('visibilitychange'))
    await vi.advanceTimersByTimeAsync(6000)
    expect(active(wrapper)).toContain('Banner 3')
  })

  it('allows keyboard, buttons and horizontal swipes, without treating vertical gestures as swipes', async () => {
    const wrapper = render()
    await wrapper.trigger('keydown', { key: 'ArrowLeft' })
    await vi.advanceTimersByTimeAsync(500)
    expect(active(wrapper)).toContain('Banner 3')
    await wrapper.find('[aria-label="Show banner 1"]').trigger('click')
    await vi.advanceTimersByTimeAsync(500)
    await wrapper.trigger('touchstart', { touches: [{ clientX: 200, clientY: 0 }] })
    await wrapper.trigger('touchend', { changedTouches: [{ clientX: 80, clientY: 20 }] })
    await vi.advanceTimersByTimeAsync(500)
    expect(active(wrapper)).toContain('Banner 2')
    await wrapper.trigger('touchstart', { touches: [{ clientX: 200, clientY: 0 }] })
    await wrapper.trigger('touchend', { changedTouches: [{ clientX: 190, clientY: 120 }] })
    expect(active(wrapper)).toContain('Banner 2')
    expect(wrapper.findAll('[inert]')).toHaveLength(2)
  })

  it('stops autoplay by preference or explicit pause and hides controls for one slide', async () => {
    const wrapper = render()
    await wrapper.find('[aria-label="Pause banners"]').trigger('click')
    await vi.advanceTimersByTimeAsync(12000)
    expect(active(wrapper)).toContain('Banner 1')
    vi.stubGlobal('matchMedia', vi.fn(() => ({ matches: true, addEventListener: vi.fn(), removeEventListener: vi.fn() })))
    const reduced = render()
    await vi.advanceTimersByTimeAsync(12000)
    expect(active(reduced)).toContain('Banner 1')
    const single = render({ slides: [slides[0]] })
    expect(single.find('.banner-carousel__controls').exists()).toBe(false)
  })

  it('keeps gradient and text when an image fails and retries a changed image URL', async () => {
    const wrapper = render()
    await wrapper.find('img').trigger('error')
    expect(wrapper.findAll('img')).toHaveLength(2)
    expect(active(wrapper)).toContain('Banner 1')
    expect(wrapper.find('.banner-carousel__gradient').exists()).toBe(true)
    await wrapper.setProps({ slides: [{ ...slides[0], image: '/replacement.png' }] })
    expect(wrapper.find('img').attributes('src')).toBe('/replacement.png')
    expect(wrapper.find('img').attributes('fetchpriority')).toBe('high')
  })

  it('recovers images that already failed before hydration attached listeners', async () => {
    vi.spyOn(HTMLImageElement.prototype, 'complete', 'get').mockReturnValue(true)
    vi.spyOn(HTMLImageElement.prototype, 'naturalWidth', 'get').mockReturnValue(0)
    const wrapper = render()
    await wrapper.vm.$nextTick()
    expect(wrapper.findAll('img')).toHaveLength(0)
    expect(active(wrapper)).toContain('Banner 1')
  })
})
