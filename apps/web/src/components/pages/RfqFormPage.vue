<script setup lang="ts">
import PageHero from '@/components/common/PageHero.vue'
import RfqForm from '@/components/forms/RfqForm.vue'
import type { PublicPageModel } from '@/types/content'
defineProps<{ page: PublicPageModel }>()
</script>
<template><main id="main-content"><PageHero :eyebrow="page.eyebrow" :title="page.title" :description="page.description" :breadcrumbs="page.breadcrumbs" compact /><RfqForm v-if="page.rfqType" :kind="page.rfqType" :product-context="page.productContext" /></main></template>
