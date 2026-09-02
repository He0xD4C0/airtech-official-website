import type { FactState } from '@/types/domain'
import type { BackendPerformanceCurve, BackendProduct, BackendTemporaryOverride } from './adminApi'

export const PRODUCT_FAMILY_LABELS: Record<BackendProduct['family'], string> = {
  centrifugal: 'Centrifugal fans',
  axial: 'Axial fans',
  crossFlow: 'Cross-flow fans',
  inlineDuct: 'Inline duct fans',
  motors: 'Motors',
}

export const PRODUCT_FAMILY_SLUGS: Record<BackendProduct['family'], string> = {
  centrifugal: 'centrifugal',
  axial: 'axial',
  crossFlow: 'cross-flow',
  inlineDuct: 'inline-duct',
  motors: 'motors',
}

export function factStatePresentation(state: FactState): {
  label: string
  tone: 'neutral' | 'success' | 'warning' | 'danger' | 'info'
} {
  return ({
    verified: { label: '已验证', tone: 'success' },
    missing: { label: '缺失', tone: 'warning' },
    notApplicable: { label: '不适用', tone: 'neutral' },
    notTested: { label: '未测试', tone: 'neutral' },
    confidential: { label: '保密', tone: 'info' },
    pendingVerification: { label: '待验证', tone: 'danger' },
  } as const)[state]
}

export function publicationStatusPresentation(status: BackendProduct['status']): {
  label: string
  tone: 'neutral' | 'success' | 'warning' | 'info'
} {
  return ({
    draft: { label: '草稿', tone: 'neutral' },
    scheduled: { label: '计划发布', tone: 'info' },
    published: { label: '已发布', tone: 'success' },
    archived: { label: '已归档', tone: 'warning' },
  } as const)[status]
}

export function formatProductValue(value: unknown): string {
  if (value === null || value === undefined || value === '') return '—'
  if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') return String(value)
  try {
    const serialized = JSON.stringify(value)
    if (!serialized) return '—'
    return serialized.length > 500 ? `${serialized.slice(0, 497)}…` : serialized
  } catch {
    return '无法显示的结构化值'
  }
}

export function formatAdminDateTime(value: string): string {
  const date = new Date(value)
  if (!Number.isFinite(date.valueOf())) return value || '—'
  return new Intl.DateTimeFormat('zh-CN', { dateStyle: 'medium', timeStyle: 'short' }).format(date)
}

export function isTemporaryOverrideExpired(override: BackendTemporaryOverride, now = Date.now()): boolean {
  const expiry = new Date(override.expiresAt).valueOf()
  return override.expired || (Number.isFinite(expiry) && expiry <= now)
}

export interface ProductPublishReadiness {
  allowed: boolean
  reason: string
}

/** The server remains authoritative; this only prevents known-invalid publish attempts in the UI. */
export function productPublishReadiness(
  product: BackendProduct,
  overrides: BackendTemporaryOverride[],
  hasPublishPermission: boolean,
  now = Date.now(),
): ProductPublishReadiness {
  if (!hasPublishPermission) return { allowed: false, reason: '当前账号没有 product.publish 权限。' }
  if (product.status === 'archived') return { allowed: false, reason: '已归档产品不能从此页面发布。' }
  if (!product.presentation) return { allowed: false, reason: '请先保存独立的网站展示草稿。' }
  if (overrides.some((override) => isTemporaryOverrideExpired(override, now))) {
    return { allowed: false, reason: '存在已到期临时覆盖；按数据治理规则必须先处理。' }
  }
  const factStates = [
    ...product.specifications.map((specification) => specification.state),
    ...product.performanceCurves.map((curve) => curve.state),
  ]
  if (factStates.includes('pendingVerification')) {
    return { allowed: false, reason: '仍有 pendingVerification 事实，需先完成验证。' }
  }
  if (!factStates.includes('verified')) {
    return { allowed: false, reason: '当前记录没有任何已验证规格或曲线。' }
  }
  const factsPublished = product.status === 'published' && product.publishedRevision === product.currentRevision
  const presentationPublished = product.presentation.publishedRevision === product.presentation.revision
  if (factsPublished && presentationPublished) {
    return { allowed: false, reason: '当前事实与网站展示 revision 均已发布。' }
  }
  return { allowed: true, reason: '可提交给 Rust API 执行最终发布校验。' }
}

export function verifiedPerformanceCurves(product: BackendProduct): BackendPerformanceCurve[] {
  return product.performanceCurves.filter((curve) => curve.state === 'verified')
}
