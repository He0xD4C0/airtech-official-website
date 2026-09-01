<script setup lang="ts">
import PageHero from '@/components/common/PageHero.vue'
import ContactForm from '@/components/forms/ContactForm.vue'
import PublishedEditorialSection from '@/components/content/PublishedEditorialSection.vue'
import type { PublicPageModel } from '@/types/content'
defineProps<{ page: PublicPageModel }>()
</script>
<template>
  <main id="main-content">
    <PageHero :eyebrow="page.eyebrow" :title="page.title" :description="page.description" :breadcrumbs="page.breadcrumbs" />
    <PublishedEditorialSection :content="page.publishedContent" />
    <section class="section shell contact-layout">
      <aside><p class="eyebrow">Choose the right path</p><h2>General question or structured inquiry?</h2><p>Use this form for routing, support, careers, supplier or general questions. For a product, selection, project or replacement request, use the RFQ path so engineering context remains structured.</p><a class="button secondary" href="/en/request-a-quote">Go to Request a Quote</a><div class="privacy-note"><strong>Data minimization</strong><p>Do not submit confidential technical information here. Form text is never copied into analytics events.</p></div></aside>
      <ContactForm />
    </section>
  </main>
</template>
