<script setup lang="ts">
import { onMounted } from 'vue'
import { storeToRefs } from 'pinia'
import { useCompareStore } from '@/features/compare/stores/compare'
import { useI18n } from 'vue-i18n'

const compare = useCompareStore()
const { locale, t } = useI18n({ useScope: 'global' })
const { items } = storeToRefs(compare)
onMounted(() => compare.hydrate())
</script>

<template>
  <aside v-if="items.length" class="compare-tray" :lang="locale" :aria-label="t('compareTray.label')">
    <div>
      <strong>{{ t('compareTray.title') }}</strong>
      <span>{{ t('compareTray.selected', { count: items.length }) }}</span>
    </div>
    <ul>
      <li v-for="item in items" :key="item.id">
        <span lang="en">{{ item.label }}</span>
        <button type="button" :aria-label="t('compareTray.remove', { label: item.label })" @click="compare.remove(item.id)">×</button>
      </li>
    </ul>
    <a class="button" href="/en/products/compare">{{ t('compareTray.action') }}</a>
  </aside>
</template>
