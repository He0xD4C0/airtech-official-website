import { describe, expect, it } from 'vitest'
import {
  activeAnalyticsPreset,
  analyticsApiRange,
  analyticsPresetAction,
  analyticsRangeForPreset,
  resolveAnalyticsDateRange,
  utcDateString,
} from './analyticsDateRange'

const NOW = new Date('2026-09-03T14:30:00Z')

describe('Analytics UTC date ranges', () => {
  it('creates inclusive absolute ranges for every preset', () => {
    expect(analyticsRangeForPreset(7, NOW)).toEqual({ from: '2026-08-28', to: '2026-09-03' })
    expect(analyticsRangeForPreset(30, NOW)).toEqual({ from: '2026-08-05', to: '2026-09-03' })
    expect(analyticsRangeForPreset(90, NOW)).toEqual({ from: '2026-06-06', to: '2026-09-03' })
  })

  it('converts the inclusive UI range to an exclusive RFC3339 API boundary', () => {
    expect(analyticsApiRange({ from: '2024-02-28', to: '2024-02-29' })).toEqual({
      from: '2024-02-28T00:00:00Z',
      to: '2024-03-01T00:00:00Z',
    })
    expect(analyticsApiRange({ from: '2025-12-31', to: '2025-12-31' })).toEqual({
      from: '2025-12-31T00:00:00Z',
      to: '2026-01-01T00:00:00Z',
    })
  })

  it('accepts a bounded absolute URL range and normalizes invalid input', () => {
    expect(resolveAnalyticsDateRange('2026-08-01', '2026-09-03', NOW)).toEqual({
      range: { from: '2026-08-01', to: '2026-09-03' },
      canonical: true,
    })
    expect(resolveAnalyticsDateRange('2026-02-30', '2026-09-03', NOW)).toEqual({
      range: { from: '2026-08-05', to: '2026-09-03' },
      canonical: false,
    })
    expect(resolveAnalyticsDateRange('2026-09-03', '2026-08-01', NOW).canonical).toBe(false)
    expect(resolveAnalyticsDateRange(['2026-08-01'], '2026-09-03', NOW).canonical).toBe(false)
    expect(resolveAnalyticsDateRange('2026-09-01', '2026-09-04', NOW)).toEqual({
      range: { from: '2026-08-05', to: '2026-09-03' },
      canonical: false,
    })
  })

  it('marks a preset active only when both absolute boundaries match', () => {
    expect(activeAnalyticsPreset({ from: '2026-08-05', to: '2026-09-03' }, NOW)).toBe(30)
    expect(activeAnalyticsPreset({ from: '2026-08-04', to: '2026-09-03' }, NOW)).toBeNull()
  })

  it('requests a reload when the selected preset already owns the URL range', () => {
    expect(analyticsPresetAction(30, { from: '2026-08-05', to: '2026-09-03' }, NOW)).toEqual({
      range: { from: '2026-08-05', to: '2026-09-03' },
      reload: true,
    })
    expect(analyticsPresetAction(7, { from: '2026-08-05', to: '2026-09-03' }, NOW).reload).toBe(false)
  })

  it('formats dates from UTC rather than the browser timezone', () => {
    expect(utcDateString(new Date('2026-09-03T23:59:59-07:00'))).toBe('2026-09-04')
  })
})
