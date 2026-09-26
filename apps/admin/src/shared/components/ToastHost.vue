<script setup lang="ts">
import { CheckCircle2, CircleAlert, Info, TriangleAlert, X } from 'lucide-vue-next'
import { useUiStore } from '@/shared/stores/ui'

const ui = useUiStore()
const icons = { success: CheckCircle2, warning: TriangleAlert, danger: CircleAlert, info: Info }
</script>

<template>
  <div class="toast-host" aria-live="polite" aria-atomic="false">
    <TransitionGroup name="toast">
      <article v-for="item in ui.toasts" :key="item.id" class="toast" :class="`toast--${item.tone}`">
        <component :is="icons[item.tone]" :size="19" aria-hidden="true" />
        <div><strong>{{ item.title }}</strong><p v-if="item.detail">{{ item.detail }}</p></div>
        <button type="button" aria-label="关闭通知" @click="ui.dismissToast(item.id)"><X :size="16" /></button>
      </article>
    </TransitionGroup>
  </div>
</template>
