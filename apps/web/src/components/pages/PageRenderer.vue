<script setup lang="ts">
import type { Component } from 'vue'
import DatabaseLandingPage from './DatabaseLandingPage.vue'
import CatalogPage from './CatalogPage.vue'
import ProductDetailPage from './ProductDetailPage.vue'
import SelectorPage from './SelectorPage.vue'
import CollectionPage from './CollectionPage.vue'
import FaqPage from './FaqPage.vue'
import DownloadsPage from './DownloadsPage.vue'
import ContactPage from './ContactPage.vue'
import RfqRouterPage from './RfqRouterPage.vue'
import RfqFormPage from './RfqFormPage.vue'
import SearchPage from './SearchPage.vue'
import PublishedContentPage from './PublishedContentPage.vue'
import ComparePage from './ComparePage.vue'
import NewsPage from './NewsPage.vue'
import NewsDetailPage from './NewsDetailPage.vue'
import type { PageKind, PublicPageModel } from '@/types/content'

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
