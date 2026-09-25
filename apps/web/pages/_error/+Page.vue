<script setup lang="ts">
import { usePageContext } from 'vike-vue/usePageContext'

const pageContext = usePageContext()
const isNotFound = pageContext.abortStatusCode === 404

function reload(): void {
  window.location.reload()
}
</script>

<template>
  <main id="main-content" class="error-page shell section">
    <p class="eyebrow">{{ isNotFound ? '404' : 'Service unavailable' }}</p>
    <h1>{{ isNotFound ? 'This page is not available.' : 'The public site is temporarily unavailable.' }}</h1>
    <p v-if="isNotFound">The URL may be incorrect, unpublished or intentionally isolated from this public website.</p>
    <p v-else>The published site data could not be loaded. Please try again shortly.</p>
    <a v-if="isNotFound" class="button" href="/en">Return home</a>
    <button v-else class="button" type="button" @click="reload">Try again</button>
  </main>
</template>
