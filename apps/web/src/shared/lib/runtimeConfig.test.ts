import { describe, expect, it } from 'vitest'
import {
  configurePublicRuntime,
  publicApiBaseUrl,
  publicApiOrigin,
  runtimePublicOrigin,
} from '@/shared/lib/runtimeConfig'

describe('Public runtime configuration', () => {
  it('normalizes the exact public API prefix and canonical origin', () => {
    configurePublicRuntime({
      apiBaseUrl: 'https://api.example.test/api/public/v1/',
      publicOrigin: 'https://www.example.test/',
    })
    expect(publicApiBaseUrl()).toBe('https://api.example.test/api/public/v1')
    expect(publicApiOrigin()).toBe('https://api.example.test')
    expect(runtimePublicOrigin()).toBe('https://www.example.test')
  })

  it('rejects alternate API paths and public origins with paths', () => {
    expect(() => configurePublicRuntime({
      apiBaseUrl: 'https://api.example.test/api/admin/v1',
      publicOrigin: 'https://www.example.test',
    })).toThrow()
    expect(() => configurePublicRuntime({
      apiBaseUrl: 'https://api.example.test/api/public/v1',
      publicOrigin: 'https://www.example.test/en',
    })).toThrow()
  })
})
