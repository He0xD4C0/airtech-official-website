<script setup lang="ts">
import { computed } from 'vue'
import type { ContentBlock, ContactChannelKind } from '@airtek/contracts'
import { linkTargetHref, type PublicContentProjection } from '@/shared/types/projection'

type ContactBlockValue = Extract<ContentBlock, { type: 'contactBlock' }>

const props = defineProps<{
  block: ContactBlockValue
  projection: PublicContentProjection
}>()

const channelLabels: Record<ContactChannelKind, string> = {
  email: 'Email',
  phone: 'Phone',
  address: 'Address',
  social: 'Social',
}

const actionHref = computed(() => linkTargetHref(
  props.block.action?.target,
  props.projection.resolvedLinks,
))
</script>

<template>
  <section class="section shell contact-block">
    <h2 v-if="block.heading">{{ block.heading }}</h2>
    <ul class="contact-channels">
      <li v-for="channel in block.channels" :key="channel">{{ channelLabels[channel] }}</li>
    </ul>
    <a v-if="block.action && actionHref" class="button" :href="actionHref">{{ block.action.label }}</a>
  </section>
</template>
