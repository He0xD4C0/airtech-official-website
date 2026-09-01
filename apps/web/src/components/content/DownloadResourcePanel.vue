<script setup lang="ts">
import { computed } from 'vue'
import { trackAnalyticsEvent } from '@/lib/analytics'
import { publishedDownloadAvailability } from '@/lib/downloadResources'
import type { PublicPageModel } from '@/types/content'

const props = defineProps<{ page: PublicPageModel }>()
const availability = computed(() => publishedDownloadAvailability(props.page))
const metadata = computed(() => availability.value.metadata)
const external = computed(() => availability.value.href?.startsWith('https://') ?? false)

function trackDownload(): void {
  const downloadId = metadata.value?.downloadId
  if (!availability.value.available || !downloadId) return
  void trackAnalyticsEvent('downloadStarted', { downloadId })
}
</script>

<template>
  <section class="download-resource-panel" aria-labelledby="download-resource-title">
    <div>
      <p class="eyebrow">Controlled resource</p>
      <h2 id="download-resource-title">File details</h2>
      <p v-if="metadata?.fileDescription" class="download-file-description">{{ metadata.fileDescription }}</p>
      <dl v-if="metadata" class="download-resource-meta">
        <div v-if="metadata.resourceType"><dt>Resource type</dt><dd>{{ metadata.resourceType }}</dd></div>
        <div v-if="metadata.version"><dt>Version</dt><dd>{{ metadata.version }}</dd></div>
        <div v-if="metadata.applicableModels.length">
          <dt>Applicable models</dt>
          <dd><ul><li v-for="model in metadata.applicableModels" :key="model">{{ model }}</li></ul></dd>
        </div>
      </dl>
    </div>
    <div class="download-resource-action">
      <a
        v-if="availability.available && availability.href"
        class="button"
        :href="availability.href"
        :rel="external ? 'noopener noreferrer' : undefined"
        @click="trackDownload"
      >Download resource</a>
      <div v-else class="data-notice" role="status">
        <strong>File unavailable</strong>
        <p>{{ availability.reason }}</p>
      </div>
    </div>
  </section>
</template>
