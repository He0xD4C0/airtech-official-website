<script setup lang="ts">
withDefaults(defineProps<{
  title?: string
  text?: string
}>(), {
  title: 'Verified data required',
  text: 'This field will be populated only from an approved Product Master record or controlled product document.',
})
</script>

<template>
  <div class="data-notice" role="status">
    <span class="notice-icon" aria-hidden="true">i</span>
    <div>
      <strong>{{ title }}</strong>
      <p>{{ text }}</p>
    </div>
  </div>
</template>
