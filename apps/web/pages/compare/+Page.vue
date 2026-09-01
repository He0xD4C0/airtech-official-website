<script setup lang="ts">
import CompareWorkspace from '@/components/product/CompareWorkspace.vue'
import PageHero from '@/components/common/PageHero.vue'
</script>

<template>
  <main id="main-content">
    <PageHero
      eyebrow="Product workspace"
      title="Compare products"
      description="Compare selected, published records. Missing and incomparable values remain explicit."
      :breadcrumbs="[
        { label: 'Home', href: '/en' },
        { label: 'Products', href: '/en/products' },
        { label: 'Compare products' },
      ]"
    />
    <CompareWorkspace />
  </main>
</template>
