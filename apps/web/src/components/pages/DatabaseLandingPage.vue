<script setup lang="ts">
import PageHero from '@/components/common/PageHero.vue'
import PublishedEditorialSection from '@/components/content/PublishedEditorialSection.vue'
import CallToAction from '@/components/common/CallToAction.vue'
import PageSlotSections from './PageSlotSections.vue'
import type { PublicPageModel } from '@/types/content'

defineProps<{ page: PublicPageModel }>()
</script>

<template>
  <main id="main-content">
    <PageHero :eyebrow="page.eyebrow" :title="page.title" :description="page.description" :breadcrumbs="page.breadcrumbs" />
    <PublishedEditorialSection :content="page.publishedContent" />

    <section v-if="page.kind === 'home' && page.productFamilies?.length" class="section shell" aria-labelledby="database-product-families">
      <h2 id="database-product-families">Product families</h2>
      <div class="family-strip">
        <a v-for="family in page.productFamilies" :key="family.code" :href="`/en/products/${family.slug}`">
          <strong>{{ family.name }}</strong>
          <small v-if="family.description">{{ family.description }}</small>
        </a>
      </div>
    </section>

    <section v-if="page.entries?.length" class="section shell" aria-label="Related published content">
      <div class="card-grid collection-grid">
        <article v-for="entry in page.entries" :key="entry.href" class="card collection-card">
          <p v-if="entry.eyebrow" class="eyebrow">{{ entry.eyebrow }}</p>
          <h2><a :href="entry.href">{{ entry.title }}</a></h2>
          <p v-if="entry.summary">{{ entry.summary }}</p>
        </article>
      </div>
    </section>

    <PageSlotSections :sections="page.sections" />
    <CallToAction
      v-if="page.primaryCta"
      :eyebrow="page.primaryCta.eyebrow"
      :title="page.primaryCta.title"
      :description="page.primaryCta.description"
      :href="page.primaryCta.href"
      :label="page.primaryCta.label"
    />
  </main>
</template>
