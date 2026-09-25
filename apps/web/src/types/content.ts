import type { ContentBlock, Product, ProductContext, ProductSourceAsset } from '@airtek/contracts'
import type { PublicContentProjection } from '@/types/projection'

export type PageKind =
  | 'home'
  | 'catalog'
  | 'product-detail'
  | 'selector'
  | 'collection'
  | 'detail'
  | 'faq'
  | 'downloads'
  | 'about'
  | 'contact'
  | 'rfq-router'
  | 'rfq-form'
  | 'search'
  | 'legal'
  | 'compare'
  | 'news'
  | 'news-detail'

export interface Breadcrumb {
  label: string
  href?: string
}

export interface CardEntry {
  slug: string
  title: string
  summary: string
  eyebrow?: string
  href: string
  status?: string
  tags?: string[]
  category?: string
  author?: string
  publishedAt?: string
  featured?: boolean
}

export interface PublicLink {
  label: string
  href: string
}

export interface PublicCallToAction extends PublicLink {
  eyebrow?: string
  title: string
  description: string
}

export interface PublicPageSection {
  id: string
  eyebrow?: string
  title?: string
  description?: string
  links?: PublicLink[]
}

export interface PublicAnalyticsContext {
  contentKind: 'content' | 'news' | 'product'
  contentId: string
  publishedRevision: number
}

export interface ProductFamilyProjection {
  code: 'centrifugal' | 'axial' | 'crossFlow' | 'inlineDuct' | 'motors'
  slug: string
  name: string
  description: string
  sortOrder: number
}

export interface PublicSiteBootstrap {
  brandName: string
  brandLine?: string
  homePath: string
  footerStatement?: string
  copyrightText?: string
  defaultSeo: {
    title?: string
    description?: string
  }
  organization: {
    name: string
    url?: string
    logoUrl?: string
  }
  siteIcon?: { url: string; mediaType: string; width: number; height: number }
  navigation: PublicLink[]
  navigationCta?: PublicLink
  footerColumns: Array<{ title: string; links: PublicLink[] }>
  legalLinks: PublicLink[]
  productFamilies: ProductFamilyProjection[]
  motorTechnologies: string[]
  generatedAt: string
  publishedRevision: number
  isPlaceholder: boolean
}

export interface PublicNewsMetadata {
  category?: string
  author?: string
  coverMediaId?: string
  publishedAt?: string
  featured: boolean
}

export interface PublicPageModel {
  kind: PageKind
  canonicalPath: string
  title: string
  metaTitle: string
  description: string
  eyebrow: string
  breadcrumbs: Breadcrumb[]
  indexable: boolean
  collection?: 'solutions' | 'technology' | 'articles' | 'cases' | 'downloads'
  slug?: string
  category?: string
  rfqType?: 'product' | 'selection' | 'project' | 'replacement'
  entries?: CardEntry[]
  dataState?: 'published' | 'placeholder'
  placeholderReason?: string
  projection?: PublicContentProjection
  blocks?: ContentBlock[]
  publishedProducts?: Product[]
  productNextCursor?: string | null
  newsNextCursor?: string | null
  publishedProduct?: Product
  productAssets?: ProductSourceAsset[]
  productContext?: ProductContext
  primaryCta?: PublicCallToAction
  sections?: PublicPageSection[]
  productFamilies?: ProductFamilyProjection[]
  motorTechnologies?: string[]
  analyticsContext?: PublicAnalyticsContext
  newsMetadata?: PublicNewsMetadata
  dataClass?: 'editorial' | 'feishu' | 'verifiedCsv' | 'developmentFixture'
}

export type RfqType = NonNullable<PublicPageModel['rfqType']>
