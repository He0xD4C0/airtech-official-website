export const ANALYTICS_PRESET_DAYS = [7, 30, 90] as const

export type AnalyticsPresetDays = (typeof ANALYTICS_PRESET_DAYS)[number]

export interface AnalyticsDateRange {
  from: string
  to: string
}

export interface AnalyticsApiRange {
  from: string
  to: string
}

export interface AnalyticsPresetAction {
  range: AnalyticsDateRange
  reload: boolean
}

const DAY_IN_MILLISECONDS = 86_400_000
const MAXIMUM_RANGE_DAYS = 366
const DATE_PATTERN = /^(\d{4})-(\d{2})-(\d{2})$/

function queryString(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined
}

function dateFromUtcString(value: string): Date | null {
  const match = DATE_PATTERN.exec(value)
  if (!match) return null

  const year = Number(match[1])
  const month = Number(match[2])
  const day = Number(match[3])
  const date = new Date(Date.UTC(year, month - 1, day))
  if (
    date.getUTCFullYear() !== year
    || date.getUTCMonth() !== month - 1
    || date.getUTCDate() !== day
  ) return null

  return date
}

function addUtcDays(value: string, days: number): string {
  const date = dateFromUtcString(value)
  if (!date) throw new TypeError(`Invalid UTC date: ${value}`)
  date.setUTCDate(date.getUTCDate() + days)
  return utcDateString(date)
}

function inclusiveDayCount(range: AnalyticsDateRange): number {
  const from = dateFromUtcString(range.from)
  const to = dateFromUtcString(range.to)
  if (!from || !to) return 0
  return Math.floor((to.getTime() - from.getTime()) / DAY_IN_MILLISECONDS) + 1
}

export function utcDateString(date: Date): string {
  const year = date.getUTCFullYear()
  const month = String(date.getUTCMonth() + 1).padStart(2, '0')
  const day = String(date.getUTCDate()).padStart(2, '0')
  return `${year}-${month}-${day}`
}

export function analyticsRangeForPreset(
  days: AnalyticsPresetDays,
  now = new Date(),
): AnalyticsDateRange {
  if (!ANALYTICS_PRESET_DAYS.includes(days)) {
    throw new RangeError('Analytics preset must be 7, 30, or 90 days.')
  }
  const to = utcDateString(now)
  return { from: addUtcDays(to, -(days - 1)), to }
}

export function resolveAnalyticsDateRange(
  fromQuery: unknown,
  toQuery: unknown,
  now = new Date(),
): { range: AnalyticsDateRange; canonical: boolean } {
  const from = queryString(fromQuery)
  const to = queryString(toQuery)
  const fromDate = from ? dateFromUtcString(from) : null
  const toDate = to ? dateFromUtcString(to) : null
  const today = dateFromUtcString(utcDateString(now))
  if (from && to && fromDate && toDate && today && toDate <= today) {
    const range = { from, to }
    const days = inclusiveDayCount(range)
    if (days >= 1 && days <= MAXIMUM_RANGE_DAYS) return { range, canonical: true }
  }

  return { range: analyticsRangeForPreset(30, now), canonical: false }
}

export function analyticsPresetAction(
  days: AnalyticsPresetDays,
  currentRange: AnalyticsDateRange,
  now = new Date(),
): AnalyticsPresetAction {
  const range = analyticsRangeForPreset(days, now)
  return {
    range,
    reload: range.from === currentRange.from && range.to === currentRange.to,
  }
}

export function activeAnalyticsPreset(
  range: AnalyticsDateRange,
  now = new Date(),
): AnalyticsPresetDays | null {
  return ANALYTICS_PRESET_DAYS.find((days) => {
    const preset = analyticsRangeForPreset(days, now)
    return preset.from === range.from && preset.to === range.to
  }) ?? null
}

export function analyticsApiRange(range: AnalyticsDateRange): AnalyticsApiRange {
  const resolved = resolveAnalyticsDateRange(range.from, range.to)
  if (!resolved.canonical) throw new TypeError('A valid Analytics date range is required.')
  return {
    from: `${range.from}T00:00:00Z`,
    to: `${addUtcDays(range.to, 1)}T00:00:00Z`,
  }
}

export function analyticsRangeLabel(range: AnalyticsDateRange): string {
  const formatter = new Intl.DateTimeFormat('zh-CN', {
    timeZone: 'UTC',
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  })
  const from = dateFromUtcString(range.from)
  const to = dateFromUtcString(range.to)
  if (!from || !to) return ''
  return `${formatter.format(from)} – ${formatter.format(to)}`
}
