<script setup lang="ts">
import PageHero from '@/components/common/PageHero.vue'
import SelectorWorkspace from '@/components/product/SelectorWorkspace.vue'
import type { PublicPageModel } from '@/types/content'
defineProps<{ page: PublicPageModel }>()
</script>
<template><main id="main-content"><PageHero :eyebrow="page.eyebrow" :title="page.title" :description="page.description" :breadcrumbs="page.breadcrumbs"><p class="hero-note">The selector never invents candidates or match percentages when validated product curves are unavailable.</p></PageHero><SelectorWorkspace /></main></template>
