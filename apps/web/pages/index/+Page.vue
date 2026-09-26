<script setup lang="ts">
import { useI18n } from 'vue-i18n'
const { locale, t } = useI18n({ useScope: 'global' })
</script>
<template>
  <main class="shell section" :lang="locale">{{ t('errors.redirectingEnglish') }}</main>
</template>
