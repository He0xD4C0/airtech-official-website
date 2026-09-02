import { describe, expect, it } from 'vitest'
import type { BackendProduct, BackendTemporaryOverride } from './adminApi'
import {
  factStatePresentation,
  formatProductValue,
  isTemporaryOverrideExpired,
  productPublishReadiness,
  verifiedPerformanceCurves,
} from './productPresentation'

function product(overrides: Partial<BackendProduct> = {}): BackendProduct {
  return {
    id: '10000000-0000-4000-8000-000000000001',
    stableId: 'stable-record-1',
    model: null,
    slug: 'stable-record-1',
    locale: 'en',
    family: 'centrifugal',
    subtype: null,
    motorTechnology: null,
    title: 'Validated API record',
    summary: null,
    status: 'draft',
    specifications: [{
      key: 'airflow', label: 'Airflow', value: 100, unit: 'm3/h', operatingCondition: 'Recorded condition',
      state: 'verified', sourceReference: 'source-cell',
    }],
    performanceCurves: [],
    sourceSnapshotId: '20000000-0000-4000-8000-000000000001',
    sourceRevision: 'source-revision-1',
    currentRevision: 2,
    publishedRevision: 1,
    indexable: false,
    seo: { title: null, description: null, canonicalPath: null, indexable: false },
    sortOrder: 0,
    relatedContentIds: [],
    updatedAt: '2026-08-31T00:00:00Z',
    presentation: {
      locale: 'en',
      slug: 'validated-api-record',
      title: 'Validated API record',
      summary: null,
      seo: { title: null, description: null, canonicalPath: null, indexable: false },
      indexable: false,
      sortOrder: 0,
      relatedContentIds: [],
      revision: 1,
      publishedRevision: null,
      updatedAt: '2026-08-31T00:00:00Z',
    },
    ...overrides,
  }
}

function temporaryOverride(overrides: Partial<BackendTemporaryOverride> = {}): BackendTemporaryOverride {
  return {
    id: '30000000-0000-4000-8000-000000000001',
    productId: '10000000-0000-4000-8000-000000000001',
    fieldPath: 'specifications.airflow',
    value: 100,
    reason: 'Controlled temporary exception',
    createdAt: '2026-08-01T00:00:00Z',
    expiresAt: '2026-09-30T00:00:00Z',
    expired: false,
    ...overrides,
  }
}

describe('product detail presentation', () => {
  it('allows only a permissioned record with verified evidence and no known blocker', () => {
    expect(productPublishReadiness(product(), [], true, Date.parse('2026-09-01T00:00:00Z'))).toEqual({
      allowed: true,
      reason: '可提交给 Rust API 执行最终发布校验。',
    })
    expect(productPublishReadiness(product(), [], false).reason).toContain('product.publish')
    expect(productPublishReadiness(product({ specifications: [] }), [], true).reason).toContain('没有任何已验证')
    expect(productPublishReadiness(product({ specifications: [{
      key: 'airflow', label: 'Airflow', value: null, unit: null, operatingCondition: null,
      state: 'pendingVerification', sourceReference: null,
    }] }), [], true).reason).toContain('pendingVerification')
    expect(productPublishReadiness(product({
      status: 'published',
      publishedRevision: 2,
      presentation: { ...product().presentation!, publishedRevision: 1 },
    }), [], true).reason).toContain('已发布')
    expect(productPublishReadiness(product({
      status: 'published',
      publishedRevision: 2,
      presentation: { ...product().presentation!, revision: 2, publishedRevision: 1 },
    }), [], true).allowed).toBe(true)
  })

  it('treats either the server flag or elapsed expiry as an expired override', () => {
    const now = Date.parse('2026-09-01T00:00:00Z')
    expect(isTemporaryOverrideExpired(temporaryOverride({ expired: true }), now)).toBe(true)
    expect(isTemporaryOverrideExpired(temporaryOverride({ expiresAt: '2026-08-31T23:59:59Z' }), now)).toBe(true)
    expect(isTemporaryOverrideExpired(temporaryOverride(), now)).toBe(false)
    expect(productPublishReadiness(product(), [temporaryOverride({ expired: true })], true, now).reason).toContain('已到期')
  })

  it('shows only API curves explicitly marked verified', () => {
    const baseCurve = {
      airflowUnit: 'm3/h', pressureUnit: 'Pa', speedRpm: null, densityKgM3: null, voltage: null,
      testMethod: null, sourceReference: 'curve-source', points: [{ airflow: 0, pressure: 0 }],
    }
    const record = product({ performanceCurves: [
      { ...baseCurve, state: 'verified' },
      { ...baseCurve, sourceReference: 'pending-source', state: 'pendingVerification' },
    ] })
    expect(verifiedPerformanceCurves(record)).toHaveLength(1)
    expect(verifiedPerformanceCurves(record)[0]?.sourceReference).toBe('curve-source')
  })

  it('presents fact states and structured values without interpreting engineering data', () => {
    expect(factStatePresentation('confidential')).toEqual({ label: '保密', tone: 'info' })
    expect(formatProductValue({ raw: [1, 2] })).toBe('{"raw":[1,2]}')
    expect(formatProductValue(null)).toBe('—')
  })
})
