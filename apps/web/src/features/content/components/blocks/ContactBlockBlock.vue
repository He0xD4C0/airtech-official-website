<script setup lang="ts">
import { computed } from 'vue'
import type { ContentBlock, ContactChannelKind } from '@airtek/contracts'
import { linkTargetHref, type PublicContentProjection } from '@/shared/types/projection'
import { useI18n } from 'vue-i18n'

type ContactBlockValue = Extract<ContentBlock, { type: 'contactBlock' }>

const props = defineProps<{
  block: ContactBlockValue
  projection: PublicContentProjection
}>()

const { locale, t } = useI18n({ useScope: 'global' })
const channelLabel = (channel: ContactChannelKind) => t(`content.contactChannels.${channel}`)

const actionHref = computed(() => linkTargetHref(
  props.block.action?.target,
  props.projection.resolvedLinks,
))
</script>

<template>
  <section class="section shell contact-block" :lang="locale">
    <h2 v-if="block.heading" lang="en">{{ block.heading }}</h2>
    <ul class="contact-channels">
      <li v-for="channel in block.channels" :key="channel">{{ channelLabel(channel) }}</li>
    </ul>
    <a v-if="block.action && actionHref" class="button" :href="actionHref" lang="en">{{ block.action.label }}</a>
  </section>
</template>
