import type { ProductFamily } from '@airtek/contracts'
import type { PublicPageModel, PublicSiteBootstrap } from '@/types/content'

export interface PublicPageDataOptions {
  baseUrl?: string
  fetchImpl?: typeof fetch
  productSlug?: string
  productFamily?: ProductFamily
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
