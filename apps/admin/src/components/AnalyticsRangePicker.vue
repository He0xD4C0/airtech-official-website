<script setup lang="ts">
import { CalendarDays } from 'lucide-vue-next'
import {
  ANALYTICS_PRESET_DAYS,
  analyticsRangeLabel,
  type AnalyticsDateRange,
  type AnalyticsPresetDays,
} from '@/services/analyticsDateRange'

defineProps<{
  range: AnalyticsDateRange
  activePreset: AnalyticsPresetDays | null
  loading?: boolean
}>()

defineEmits<{ select: [days: AnalyticsPresetDays] }>()
</script>

<template>
  <section class="analytics-range-picker" aria-label="Analytics 日期范围">
    <div class="analytics-range-picker__presets" role="group" aria-label="UTC 日期预设">
      <button
        v-for="days in ANALYTICS_PRESET_DAYS"
        :key="days"
        class="button button--secondary"
        :class="{ 'is-active': activePreset === days }"
        type="button"
        :aria-pressed="activePreset === days"
        :disabled="loading"
        @click="$emit('select', days)"
      >
        最近 {{ days }} 天
      </button>
    </div>
    <p><CalendarDays :size="16" /><strong>{{ analyticsRangeLabel(range) }}</strong><span>含首含尾 · UTC</span></p>
  </section>
</template>
