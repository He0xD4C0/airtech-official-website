<script setup lang="ts">
import PageHero from '@/components/common/PageHero.vue'
import CallToAction from '@/components/common/CallToAction.vue'
import SectionHeading from '@/components/common/SectionHeading.vue'
import DataNotice from '@/components/common/DataNotice.vue'
import PublishedEditorialSection from '@/components/content/PublishedEditorialSection.vue'
import type { PublicPageModel } from '@/types/content'
defineProps<{ page: PublicPageModel }>()
</script>
<template>
  <main id="main-content">
    <PageHero :eyebrow="page.eyebrow" :title="page.title" :description="page.description" :breadcrumbs="page.breadcrumbs" />
    <PublishedEditorialSection :content="page.publishedContent" />
    <section class="section shell about-intro"><p class="eyebrow">Our purpose</p><h2>Engineering airflow decisions around clear context and responsible data.</h2><p>AIRTEKPOWER brings product discovery, technical understanding and inquiry handoff into one connected experience. Public information is separated from unverified claims and model-level records stay tied to their sources and revisions.</p></section>
    <section class="section surface-section"><div class="shell principles-grid"><article><span>Mission</span><blockquote>“To empower global industries with intelligent and energy-efficient ventilation solutions.”</blockquote></article><article><span>Vision</span><blockquote>“To become a global leader in green ventilation solutions.”</blockquote></article></div></section>
    <section class="section shell"><SectionHeading eyebrow="Values" title="Innovation · Quality · Service" description="The exact company values guide how product data, technical content and customer inquiries are handled." /><div class="three-columns"><article><h3>Innovation</h3><p>Frame engineering problems clearly and make complex technical paths easier to navigate.</p></article><article><h3>Quality</h3><p>Keep values, units, conditions, evidence and publication status visible.</p></article><article><h3>Service</h3><p>Carry useful context from discovery into a focused human follow-up.</p></article></div></section>
    <section class="section shell"><DataNotice title="Company profile review" text="Legal entity details, address, facilities, certifications and current company metrics remain held for owner verification before production publication." /></section>
    <CallToAction title="Talk through an airflow requirement" description="Choose a structured inquiry or use Contact for general routing." />
  </main>
</template>
