import { describe, expect, it } from 'vitest'
import { normalizePublicOrigin } from './publicOrigin'

describe('canonical public origin', () => {
  it('normalizes a configured origin without adding paths', () => {
    expect(normalizePublicOrigin('https://www.example.test/')).toBe('https://www.example.test')
  })

  it('fails closed for credentials, paths and non-http protocols', () => {
    expect(() => normalizePublicOrigin('https://user@example.test')).toThrow()
    expect(() => normalizePublicOrigin('https://www.example.test/en')).toThrow()
    expect(() => normalizePublicOrigin('javascript:alert(1)')).toThrow()
  })
})
