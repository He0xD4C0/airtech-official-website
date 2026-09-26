<script setup lang="ts">
import type { ContentBlock } from '@airtek/contracts'

type EvidenceBlockValue = Extract<ContentBlock, { type: 'evidence' }>

defineProps<{ block: EvidenceBlockValue }>()
</script>

<template>
  <section class="section shell evidence-block">
    <h2 v-if="block.heading">{{ block.heading }}</h2>
    <dl class="evidence-list">
      <div v-for="item in block.items" :key="item.id" class="evidence-item">
        <dt>{{ item.label }}</dt>
        <dd>
          <p>{{ item.statement }}</p>
          <small v-if="item.sourceNote">{{ item.sourceNote }}</small>
        </dd>
      </div>
    </dl>
  </section>
</template>
