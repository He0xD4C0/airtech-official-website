<script setup lang="ts">
import { AlertTriangle, Ban, Database, LoaderCircle } from 'lucide-vue-next'

withDefaults(defineProps<{
  state: 'loading' | 'empty' | 'error' | 'forbidden'
  title?: string
  description?: string
  actionLabel?: string
}>(), {
  title: '',
  description: '',
  actionLabel: '',
})

defineEmits<{ retry: []; action: [] }>()
</script>

<template>
  <section class="panel data-state" :aria-busy="state === 'loading'">
    <LoaderCircle v-if="state === 'loading'" class="data-state__spin" :size="28" />
    <Database v-else-if="state === 'empty'" :size="28" />
    <Ban v-else-if="state === 'forbidden'" :size="28" />
    <AlertTriangle v-else :size="28" />
    <h2>{{ title || (state === 'loading' ? '正在读取数据库' : state === 'empty' ? '暂无记录' : state === 'forbidden' ? '没有访问权限' : '数据读取失败') }}</h2>
    <p>{{ description || (state === 'loading' ? '正在加载最新数据。' : state === 'empty' ? '创建或导入记录后会显示在这里。' : state === 'forbidden' ? '请联系管理员分配所需权限。' : '请检查 API 状态后重试。') }}</p>
    <button v-if="state === 'error'" class="button button--secondary" type="button" @click="$emit('retry')">重新加载</button>
    <button v-else-if="state === 'empty' && actionLabel" class="button button--primary" type="button" @click="$emit('action')">{{ actionLabel }}</button>
  </section>
</template>
