import { describe, expect, it } from 'vitest'
import config from './+config'
import route from './+route'

describe('product comparison route', () => {
  it('is an isolated client-rendered, canonical public tool route', () => {
    expect(route).toBe('/en/products/compare')
    expect(config.ssr).toBe(false)
  })
})
