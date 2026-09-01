<script setup lang="ts">
import PageHero from '@/components/common/PageHero.vue'
import DataNotice from '@/components/common/DataNotice.vue'
import type { PublicPageModel } from '@/types/content'
defineProps<{ page: PublicPageModel }>()
function openCookieSettings() { window.dispatchEvent(new CustomEvent('airtek:open-cookie-settings')) }
</script>
<template>
  <main id="main-content"><PageHero :eyebrow="page.eyebrow" :title="page.title" :description="page.description" :breadcrumbs="page.breadcrumbs" compact /><section class="section shell legal-copy"><template v-if="page.slug === 'cookie-settings' || page.canonicalPath.endsWith('/cookie-settings')"><h2>Optional analytics</h2><p>Optional measurement remains off unless you accept it. Essential storage records only the preference needed to avoid asking on every visit.</p><button class="button" type="button" @click="openCookieSettings">Review cookie preference</button></template><template v-else><DataNotice title="Legal review required" text="This route is intentionally noindex until owner-approved, market-appropriate legal copy is supplied. The placeholder must not be treated as a production notice." /><h2>Publication status</h2><p>The application supports revisioned legal pages and consent-aware configuration. Final text, effective date, data-controller details and applicable rights must be reviewed before launch.</p></template></section></main>
</template>
