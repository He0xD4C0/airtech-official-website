<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

const props = defineProps<{
  title?: string
  text?: string
}>()
const { t } = useI18n({ useScope: 'global' })
const titleText = computed(() => props.title ?? t('product.dataRequired'))
const bodyText = computed(() => props.text ?? t('product.dataRequiredDescription'))
</script>

<template>
  <div class="data-notice" role="status">
    <span class="notice-icon" aria-hidden="true">i</span>
    <div>
      <strong>{{ titleText }}</strong>
      <p>{{ bodyText }}</p>
    </div>
  </div>
</template>
