<script setup lang="ts">
import { onMounted } from 'vue'
import { storeToRefs } from 'pinia'
import { useCompareStore } from '@/features/compare/stores/compare'

const compare = useCompareStore()
const { items } = storeToRefs(compare)
onMounted(() => compare.hydrate())
</script>

<template>
  <aside v-if="items.length" class="compare-tray" aria-label="Product comparison tray">
    <div>
      <strong>Compare workspace</strong>
      <span>{{ items.length }}/4 records selected</span>
    </div>
    <ul>
      <li v-for="item in items" :key="item.id">
        <span>{{ item.label }}</span>
        <button type="button" :aria-label="`Remove ${item.label} from comparison`" @click="compare.remove(item.id)">×</button>
      </li>
    </ul>
    <a class="button" href="/en/products/compare">Compare</a>
  </aside>
</template>
