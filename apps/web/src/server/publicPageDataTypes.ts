import type { ProductFamily, PublicSearchType } from '@airtek/contracts'
import type { PublicPageModel, PublicSiteBootstrap } from '@/shared/types/content'

export interface PublicPageDataOptions {
  baseUrl?: string
  fetchImpl?: typeof fetch
  productSlug?: string
  productFamily?: ProductFamily
  catalogQuery?: {
    cursor?: string
    q?: string
    family?: ProductFamily
    motorTechnology?: string
    view?: 'cards' | 'table'
  }
  searchQuery?: {
    cursor?: string
    q?: string
    type?: PublicSearchType
  }
}

export interface LoadedPublicPageData {
  page: PublicPageModel
  site: PublicSiteBootstrap
}

export class PublicPageDataError extends Error {
  constructor(message: string, readonly status: 404 | 503) {
    super(message)
    this.name = 'PublicPageDataError'
  }
}
