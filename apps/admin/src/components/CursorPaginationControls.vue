<script setup lang="ts">
defineProps<{
  itemCount: number
  pageNumber: number
  canPrevious: boolean
  canNext: boolean
  loading?: boolean
  label: string
}>()

defineEmits<{
  previous: []
  next: []
}>()
</script>

<template>
  <footer class="table-footer" aria-label="分页">
    <span aria-live="polite">第 {{ pageNumber }} 页 · 当前 {{ itemCount }} 条{{ label }}<template v-if="loading"> · 正在读取…</template></span>
    <div>
      <button type="button" :disabled="!canPrevious || loading" @click="$emit('previous')">上一页</button>
      <button type="button" :disabled="!canNext || loading" @click="$emit('next')">下一页</button>
    </div>
  </footer>
</template>
