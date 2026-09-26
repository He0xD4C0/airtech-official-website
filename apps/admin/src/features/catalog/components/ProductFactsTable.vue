<script setup lang="ts">
import StatusBadge from '@/shared/components/StatusBadge.vue'
import type { BackendSpecValue } from '@/shared/services/adminApiTypes'
import { factStatePresentation, formatProductValue } from '@/features/catalog/services/productPresentation'

defineProps<{ specifications: BackendSpecValue[] }>()
</script>

<template>
  <div v-if="specifications.length" class="data-table-wrap">
    <table class="data-table product-fact-table">
      <caption class="sr-only">产品规格、单位、工况、事实状态与来源</caption>
      <thead><tr><th scope="col">规格</th><th scope="col">值</th><th scope="col">单位</th><th scope="col">Fact state</th><th scope="col">工况</th><th scope="col">来源</th></tr></thead>
      <tbody>
        <tr v-for="specification in specifications" :key="specification.key">
          <th scope="row"><strong>{{ specification.label }}</strong><code>{{ specification.key }}</code></th>
          <td><span class="product-fact-value">{{ formatProductValue(specification.value) }}</span></td>
          <td>{{ specification.unit || '—' }}</td>
          <td><StatusBadge v-bind="factStatePresentation(specification.state)" /></td>
          <td>{{ specification.operatingCondition || '—' }}</td>
          <td>{{ specification.sourceReference || '—' }}</td>
        </tr>
      </tbody>
    </table>
  </div>
  <p v-else class="product-empty-copy">当前记录没有规格事实；本页不会生成占位参数。</p>
</template>
