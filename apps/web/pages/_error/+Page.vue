<script setup lang="ts">
import { usePageContext } from 'vike-vue/usePageContext'
import { useI18n } from 'vue-i18n'

const pageContext = usePageContext()
const { locale, t } = useI18n({ useScope: 'global' })
const isNotFound = pageContext.abortStatusCode === 404

function reload(): void {
  window.location.reload()
}
</script>

<template>
  <main id="main-content" class="error-page shell section" :lang="locale">
    <p class="eyebrow">{{ isNotFound ? '404' : t('errors.serviceUnavailable') }}</p>
    <h1>{{ isNotFound ? t('errors.notFoundTitle') : t('errors.serviceTitle') }}</h1>
    <p>{{ isNotFound ? t('errors.notFoundDescription') : t('errors.serviceDescription') }}</p>
    <a v-if="isNotFound" class="button" href="/en">{{ t('errors.returnHome') }}</a>
    <button v-else class="button" type="button" @click="reload">{{ t('errors.tryAgain') }}</button>
  </main>
</template>
