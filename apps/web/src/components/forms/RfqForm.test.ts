import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ApiError } from '@airtek/contracts'
import { submitRfq } from '@/lib/api'
import RfqForm from './RfqForm.vue'

vi.mock('@/lib/api', async () => ({
  PublicApiError: (await import('@airtek/contracts')).ApiError,
  newIdempotencyKey: () => 'test-key', submitRfq: vi.fn(),
}))
vi.mock('@/lib/analytics', () => ({ trackAnalyticsEvent: vi.fn() }))
afterEach(() => { vi.restoreAllMocks(); document.body.innerHTML = '' })

describe('RFQ field errors and optional browser storage', () => {
  it('returns to and focuses the rejected step while preserving the request', async () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => { throw new Error('blocked') })
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('blocked') })
    vi.spyOn(window, 'scrollTo').mockImplementation(() => {})
    HTMLElement.prototype.scrollIntoView = vi.fn()
    vi.mocked(submitRfq).mockRejectedValue(new ApiError({
      type: 'about:blank', title: 'Invalid', status: 422, detail: 'Check fields', requestId: 'test-request',
      errors: { 'context.projectScale': ['Please clarify scale.'] },
    }))
    const wrapper = mount(RfqForm, { props: { kind: 'project' }, attachTo: document.body })
    const input = (path: string) => wrapper.get(`[id="rfq-${path}"]`)
    await input('context.application').setValue('Preserved requirement')
    await input('context.projectStage').setValue('Engineering')
    await input('context.projectScale').setValue('Original scale')
    await wrapper.get('form').trigger('submit')
    await input('contact.name').setValue('Buyer')
    await input('contact.email').setValue('buyer@example.com')
    await input('contact.company').setValue('Example')
    await input('contact.countryOrRegion').setValue('CN')
    await input('consent').setValue(true)
    await wrapper.get('form').trigger('submit')
    expect(wrapper.text()).toContain('Country or regionCN')
    expect(wrapper.text()).not.toContain('Airflow unit')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(input('context.projectScale').element).toBe(document.activeElement)
    expect(input('context.projectScale').attributes('aria-invalid')).toBe('true')
    expect((input('context.application').element as HTMLInputElement).value).toBe('Preserved requirement')
    expect(wrapper.text()).toContain('Please clarify scale.')
    wrapper.unmount()
  })
})
