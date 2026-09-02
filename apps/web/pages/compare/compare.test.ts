import { describe, expect, it } from 'vitest'
import config from './+config'
import route from './+route'

describe('product comparison route', () => {
  it('server-renders its database projection on the canonical public tool route', () => {
    expect(route).toBe('/en/products/compare')
    expect(config.ssr).toBe(true)
  })
})
