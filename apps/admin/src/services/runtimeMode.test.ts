import { describe, expect, it } from 'vitest'
import { shouldUseMockApi } from './runtimeMode'

describe('Admin mock API boundary', () => {
  it('is disabled by default during development', () => {
    expect(shouldUseMockApi(true, undefined)).toBe(false)
    expect(shouldUseMockApi(true, 'false')).toBe(false)
  })

  it('requires an exact development-only opt in', () => {
    expect(shouldUseMockApi(true, 'true')).toBe(true)
    expect(shouldUseMockApi(true, 'TRUE')).toBe(false)
    expect(shouldUseMockApi(false, 'true')).toBe(false)
  })
})
