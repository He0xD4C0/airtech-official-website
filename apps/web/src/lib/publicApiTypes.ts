import type {
  CreateGuestVisit,
  DiscoveryDocument,
  DiscoveryEntry,
  GuestVisit,
  NewsEntry,
  NewsPage,
  ProductFamilyPresentation,
  ProductSourceAssetDocument,
  RouteResolution,
  SiteBootstrap,
  operations,
} from '@airtek/contracts'

export interface PublicApiClientOptions {
  baseUrl: string
  fetchImpl?: typeof fetch
}

export type PublishedProductListQuery = NonNullable<
  operations['listPublishedProducts']['parameters']['query']
>
export type PublishedNewsListQuery = NonNullable<
  operations['listPublishedNews']['parameters']['query']
>
export type ProductFamilyProjectionResponse = ProductFamilyPresentation
export type SiteBootstrapResponse = SiteBootstrap
export type RouteProjectionResponse = RouteResolution
export type NewsEntryResponse = NewsEntry
export type NewsPageResponse = NewsPage
export type PublicDiscoveryEntryResponse = DiscoveryEntry
export type PublicDiscoveryResponse = DiscoveryDocument
export type GuestVisitRequest = CreateGuestVisit
export type GuestVisitResponse = GuestVisit
export type ProductSourceAssetDocumentResponse = ProductSourceAssetDocument
