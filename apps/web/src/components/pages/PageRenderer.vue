<script setup lang="ts">
import { computed } from 'vue'
import HomePage from './HomePage.vue'
import CatalogPage from './CatalogPage.vue'
import ProductDetailPage from './ProductDetailPage.vue'
import SelectorPage from './SelectorPage.vue'
import CollectionPage from './CollectionPage.vue'
import DetailPage from './DetailPage.vue'
import FaqPage from './FaqPage.vue'
import DownloadsPage from './DownloadsPage.vue'
import AboutPage from './AboutPage.vue'
import ContactPage from './ContactPage.vue'
import RfqRouterPage from './RfqRouterPage.vue'
import RfqFormPage from './RfqFormPage.vue'
import SearchPage from './SearchPage.vue'
import LegalPage from './LegalPage.vue'
import PublishedContentPage from './PublishedContentPage.vue'
import ReservedContentPage from './ReservedContentPage.vue'
import DataNotice from '@/components/common/DataNotice.vue'
import { hasRenderableRichText } from '@/lib/richText'
import type { PublicPageModel } from '@/types/content'

const props = defineProps<{ page: PublicPageModel }>()
const pages = {
  home: HomePage, catalog: CatalogPage, 'product-detail': ProductDetailPage, selector: SelectorPage,
  collection: CollectionPage, detail: DetailPage, faq: FaqPage, downloads: DownloadsPage,
  about: AboutPage, contact: ContactPage, 'rfq-router': RfqRouterPage, 'rfq-form': RfqFormPage,
  search: SearchPage, legal: LegalPage,
}

// Detail records and legal notices use the CMS document as their primary page.
// Route-specific pages compose the same document inside their own core template
// so publishing copy cannot remove discovery, filtering or form components.
const publishedKinds = new Set(['detail', 'legal'])
const usePublishedBody = computed(() => (
  publishedKinds.has(props.page.kind)
  && props.page.publishedContent?.status === 'published'
  && hasRenderableRichText(props.page.publishedContent.body)
))
</script>
<template>
  <div v-if="page.dataState === 'placeholder' && page.placeholderReason" class="shell projection-status">
    <DataNotice title="Published data unavailable" :text="page.placeholderReason" />
  </div>
  <PublishedContentPage v-if="usePublishedBody" :page="page" />
  <ReservedContentPage v-else-if="page.requiresPublishedContent" :page="page" />
  <component :is="pages[page.kind]" v-else :page="page" />
</template>
