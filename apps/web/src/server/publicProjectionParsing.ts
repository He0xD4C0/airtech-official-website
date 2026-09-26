import type { SiteBootstrapResponse } from '@/shared/lib/publicApiTypes'
import type { ProductFamilyProjection, PublicSiteBootstrap } from '@/shared/types/content'
import { PublicPageDataError } from '@/server/publicPageDataTypes'
import {
  footerColumnsFrom,
  generalInformationFrom,
  legalLinksFrom,
  navigationLinksFrom,
} from '@/features/content/lib/publicProjectionV2'

const unsafeControls = /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/u
export const slugPattern = /^[a-z0-9]+(?:-[a-z0-9]+)*$/u

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

export function safeText(value: unknown, maxLength = 1_000): string | undefined {
  if (typeof value !== 'string' || value.length > maxLength || unsafeControls.test(value)) return undefined
  const result = value.trim()
  return result || undefined
}

export function parseSiteBootstrap(value: SiteBootstrapResponse): PublicSiteBootstrap {
  const informationProjection = value.generalInformation
  const navigationProjection = value.navigation
  const footerProjection = value.footer
  if (!informationProjection || !navigationProjection || !footerProjection) {
    throw new PublicPageDataError('The public site bootstrap has no complete CMS V2 projection.', 503)
  }
  if (informationProjection.kind !== 'generalInformation'
    || informationProjection.templateKey !== 'generalInformation'
    || informationProjection.typeFields.type !== 'generalInformation'
    || navigationProjection.kind !== 'navigation'
    || navigationProjection.templateKey !== 'navigation'
    || navigationProjection.typeFields.type !== 'navigation'
    || footerProjection.kind !== 'footer'
    || footerProjection.templateKey !== 'footer'
    || footerProjection.typeFields.type !== 'footer') {
    throw new PublicPageDataError('The public site bootstrap contains mismatched singleton projections.', 503)
  }
  const information = generalInformationFrom(informationProjection)
  if (!information) {
    throw new PublicPageDataError('The published General Information projection is incomplete.', 503)
  }
  return {
    brandName: information.brandName,
    brandLine: information.brandLine,
    homePath: information.homePath,
    footerStatement: information.footerStatement,
    copyrightText: information.copyrightText,
    defaultSeo: information.defaultSeo,
    organization: information.organization,
    siteIcon: information.siteIcon,
    navigation: navigationLinksFrom(navigationProjection),
    navigationCta: information.navigationCta,
    footerColumns: footerColumnsFrom(footerProjection),
    legalLinks: legalLinksFrom(footerProjection),
    productFamilies: value.productFamilies
      .map((family): ProductFamilyProjection => ({ ...family }))
      .sort((left, right) => left.sortOrder - right.sortOrder),
    motorTechnologies: value.motorTechnologies,
    generatedAt: value.generatedAt,
    publishedRevision: informationProjection.publishedRevision,
    isPlaceholder: informationProjection.isPlaceholder
      || navigationProjection.isPlaceholder
      || footerProjection.isPlaceholder,
  }
}
