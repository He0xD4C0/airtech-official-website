import { describe, expect, it } from 'vitest'
import { routeComponentKey } from '@/app/router/routeComponentKey'

describe('routeComponentKey', () => {
  it('separates named routes that reuse the same Vue component', () => {
    expect(routeComponentKey({ name: 'site-navigation', path: '/site/navigation' }))
      .not.toBe(routeComponentKey({ name: 'site-footer', path: '/site/footer' }))
    expect(routeComponentKey({ name: 'settings', path: '/settings' }))
      .not.toBe(routeComponentKey({ name: 'account-security', path: '/account/security' }))
  })

  it('keeps one component instance for sections handled inside the same named route', () => {
    expect(routeComponentKey({ name: 'settings', path: '/settings/general' }))
      .toBe(routeComponentKey({ name: 'settings', path: '/settings/security' }))
  })

  it('falls back to the path for unnamed routes', () => {
    expect(routeComponentKey({ name: undefined, path: '/fallback' })).toBe('/fallback')
  })
})
