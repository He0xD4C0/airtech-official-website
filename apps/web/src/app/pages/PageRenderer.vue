<script setup lang="ts">
import type { Component } from 'vue'
import DatabaseLandingPage from '@/features/content/pages/DatabaseLandingPage.vue'
import CatalogPage from '@/features/catalog/pages/CatalogPage.vue'
import ProductDetailPage from '@/features/catalog/pages/ProductDetailPage.vue'
import SelectorPage from '@/features/selector/pages/SelectorPage.vue'
import CollectionPage from '@/features/content/pages/CollectionPage.vue'
import FaqPage from '@/features/content/pages/FaqPage.vue'
import DownloadsPage from '@/features/content/pages/DownloadsPage.vue'
import ContactPage from '@/features/conversion/pages/ContactPage.vue'
import RfqRouterPage from '@/features/conversion/pages/RfqRouterPage.vue'
import RfqFormPage from '@/features/conversion/pages/RfqFormPage.vue'
import SearchPage from '@/features/search/pages/SearchPage.vue'
import PublishedContentPage from '@/features/content/pages/PublishedContentPage.vue'
import ComparePage from '@/features/compare/pages/ComparePage.vue'
import NewsPage from '@/features/content/pages/NewsPage.vue'
import NewsDetailPage from '@/features/content/pages/NewsDetailPage.vue'
import type { PageKind, PublicPageModel } from '@/shared/types/content'

defineProps<{ page: PublicPageModel }>()
const pages: Record<PageKind, Component> = {
  home: DatabaseLandingPage,
  catalog: CatalogPage,
  'product-detail': ProductDetailPage,
  selector: SelectorPage,
  collection: CollectionPage,
  detail: PublishedContentPage,
  faq: FaqPage,
  downloads: DownloadsPage,
  about: DatabaseLandingPage,
  contact: ContactPage,
  'rfq-router': RfqRouterPage,
  'rfq-form': RfqFormPage,
  search: SearchPage,
  legal: PublishedContentPage,
  compare: ComparePage,
  news: NewsPage,
  'news-detail': NewsDetailPage,
}
</script>
<template>
  <component :is="pages[page.kind]" :page="page" />
</template>
