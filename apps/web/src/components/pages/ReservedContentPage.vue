<script setup lang="ts">
import PageHero from '@/components/common/PageHero.vue'
import type { PublicPageModel } from '@/types/content'

defineProps<{ page: PublicPageModel }>()
</script>

<template>
  <main id="main-content">
    <PageHero :eyebrow="page.eyebrow" :title="page.title" :description="page.description" :breadcrumbs="page.breadcrumbs" />
    <section class="section shell">
      <div class="empty-state large">
        <p class="eyebrow">Publication required</p>
        <h2>This route has no published content.</h2>
        <p>It remains excluded from indexing and structured data until a validated CMS revision is published.</p>
        <a class="button secondary" href="/en">Return home</a>
      </div>
    </section>
  </main>
</template>
