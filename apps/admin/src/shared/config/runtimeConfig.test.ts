import { describe, expect, it, vi } from 'vitest'
import { adminApiBaseUrl, adminApiOrigin, configureAdminRuntime } from '@/shared/config/runtimeConfig'

describe('Admin runtime configuration', () => {
  it('accepts the exact public Admin API prefix', () => {
    configureAdminRuntime({ apiBaseUrl: 'https://api.example.test/api/admin/v1/' })
    expect(adminApiBaseUrl()).toBe('https://api.example.test/api/admin/v1')
    expect(adminApiOrigin()).toBe('https://api.example.test')
  })

  it('rejects credentials, query strings, and alternate paths', () => {
    for (const apiBaseUrl of [
      'https://user@api.example.test/api/admin/v1',
      'https://api.example.test/api/public/v1',
      'https://api.example.test/api/admin/v1?debug=true',
    ]) {
      expect(() => configureAdminRuntime({ apiBaseUrl })).toThrow()
    }
    vi.restoreAllMocks()
  })
})
