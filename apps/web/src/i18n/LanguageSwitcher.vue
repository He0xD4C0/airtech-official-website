<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  persistPublicUiLocale,
  storedPublicUiLocale,
  type PublicUiLocale,
} from '@/i18n'

const { locale, t } = useI18n({ useScope: 'global' })
const currentLocale = computed({
  get: () => locale.value as PublicUiLocale,
  set: (value: PublicUiLocale) => {
    locale.value = value
    persistPublicUiLocale(window.localStorage, value)
  },
})

onMounted(() => {
  currentLocale.value = storedPublicUiLocale(window.localStorage)
})
</script>

<template>
  <label class="language-switcher" :lang="currentLocale">
    <span>{{ t('navigation.interfaceLanguage') }}</span>
    <select v-model="currentLocale" :aria-label="t('navigation.interfaceLanguage')">
      <option value="en">{{ t('navigation.english') }}</option>
      <option value="zh-CN">{{ t('navigation.simplifiedChinese') }}</option>
    </select>
  </label>
</template>
