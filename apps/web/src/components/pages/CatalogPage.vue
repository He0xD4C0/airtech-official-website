<script setup lang="ts">
import PageHero from '@/components/common/PageHero.vue'
import ProductExplorer from '@/components/product/ProductExplorer.vue'
import CallToAction from '@/components/common/CallToAction.vue'
import type { PublicPageModel } from '@/types/content'

defineProps<{ page: PublicPageModel }>()
</script>

<template>
  <main id="main-content">
    <PageHero :eyebrow="page.eyebrow" :title="page.title" :description="page.description" :breadcrumbs="page.breadcrumbs">
      <div class="button-row"><a class="button" href="/en/products/selector">Open Fan Selector</a><a class="button secondary" href="/en/products/compare">Compare products</a></div>
    </PageHero>
    <ProductExplorer :category="page.category" :products="page.publishedProducts ?? []" :next-cursor="page.productNextCursor" />
    <section class="section surface-section">
      <div class="shell three-columns">
        <article><p class="eyebrow">01</p><h2>Product identity</h2><p>Every published model keeps a stable ID, exact model string, revision and lifecycle state.</p></article>
        <article><p class="eyebrow">02</p><h2>Operating context</h2><p>Units, speed, voltage, test setup and environmental conditions travel with performance data.</p></article>
        <article><p class="eyebrow">03</p><h2>Explicit gaps</h2><p>Missing, not applicable, not tested, confidential and pending verification are distinct states.</p></article>
      </div>
    </section>
    <CallToAction title="Cannot identify a published model yet?" description="Capture the duty point and constraints for engineering review." href="/en/request-a-quote/selection" label="Start Selection RFQ" />
  </main>
</template>
