<script setup lang="ts">
import ContactForm from '@/components/forms/ContactForm.vue'
import PublicBlockRenderer from '@/components/blocks/PublicBlockRenderer.vue'
import type { PublicPageModel } from '@/types/content'
defineProps<{ page: PublicPageModel }>()
</script>
<template>
  <main id="main-content">
    <PublicBlockRenderer
      v-if="page.projection"
      :blocks="page.projection.composition.blocks"
      :projection="page.projection"
      :breadcrumbs="page.breadcrumbs"
    />
    <section class="section shell">
      <ContactForm />
    </section>
  </main>
</template>
