<script setup lang="ts">
import { usePageContext } from 'vike-vue/usePageContext'

const pageContext = usePageContext()
</script>

<template>
  <main id="main-content" class="error-page shell section">
    <p class="eyebrow">{{ pageContext.abortStatusCode === 404 ? '404' : 'Request error' }}</p>
    <h1>{{ pageContext.abortStatusCode === 404 ? 'This page is not available.' : 'We could not load this page.' }}</h1>
    <p>The URL may be incorrect, unpublished or intentionally isolated from this public website.</p>
    <a class="button" href="/en">Return home</a>
  </main>
</template>
