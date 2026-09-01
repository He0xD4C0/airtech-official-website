import type { ContentEntry, Product, ProductContext } from '@airtek/contracts'
import type { PublishedDownloadListMetadata } from '@/lib/downloadResources'

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
  download?: PublishedDownloadListMetadata
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
  requiresPublishedContent?: boolean
  publishedContent?: ContentEntry
  publishedProducts?: Product[]
  productNextCursor?: string | null
  publishedProduct?: Product
  productContext?: ProductContext
}

export interface ProductFamily {
  id: string
  name: string
  slug: string
  description: string
  form: string
  subtypes?: string[]
}

export type RfqType = NonNullable<PublicPageModel['rfqType']>
