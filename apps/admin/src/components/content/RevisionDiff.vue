<script setup lang="ts">
import { computed, ref } from 'vue'
import { AlertTriangle, ArrowRight, ChevronDown, ChevronRight, LoaderCircle, Search } from 'lucide-vue-next'

interface DiffEntry {
  path: string
  before: unknown
  after: unknown
}

const props = withDefaults(defineProps<{
  changes: readonly DiffEntry[]
  loading?: boolean
  error?: string
  emptyMessage?: string
}>(), {
  loading: false,
  error: '',
  emptyMessage: '没有差异。',
})

const filter = ref('')
const expanded = ref<Set<string>>(new Set())

const visible = computed(() => {
  const needle = filter.value.trim().toLowerCase()
  if (!needle) return props.changes
  return props.changes.filter((change) => change.path.toLowerCase().includes(needle))
})

function formatValue(value: unknown): string {
  if (value === undefined) return '∅ 未设置'
  if (value === null) return 'null'
  if (typeof value === 'string') return value.length ? value : '""（空字符串）'
  try {
    return JSON.stringify(value)
  } catch {
    return String(value)
  }
}

function isLong(value: unknown): boolean {
  return formatValue(value).length > 120
}

function toggle(path: string): void {
  const next = new Set(expanded.value)
  if (next.has(path)) next.delete(path)
  else next.add(path)
  expanded.value = next
}
</script>

<template>
  <section class="revision-diff" aria-labelledby="revision-diff-title">
    <header class="revision-diff__header">
      <h3 id="revision-diff-title">字段差异</h3>
      <span v-if="!loading && !error" class="revision-diff__count">{{ changes.length }} 处差异</span>
    </header>

    <p v-if="loading" class="revision-diff__state">
      <LoaderCircle class="data-state__spin" :size="15" />正在计算差异…
    </p>
    <p v-else-if="error" class="revision-diff__state revision-diff__state--error" role="alert">
      <AlertTriangle :size="15" />{{ error }}
    </p>
    <p v-else-if="!changes.length" class="revision-diff__state">{{ emptyMessage }}</p>
    <template v-else>
      <label class="search-field revision-diff__filter">
        <Search :size="15" />
        <span class="sr-only">按字段路径筛选差异</span>
        <input v-model="filter" type="search" placeholder="按字段路径筛选，例如 composition.blocks" />
      </label>
      <p v-if="!visible.length" class="revision-diff__state">没有匹配该路径的差异。</p>
      <ul v-else class="revision-diff__list">
        <li v-for="change in visible" :key="change.path">
          <p class="revision-diff__path"><code>{{ change.path }}</code></p>
          <div class="revision-diff__values">
            <div class="revision-diff__value revision-diff__value--before">
              <span>修改前</span>
              <p :class="{ 'is-clamped': !expanded.has(change.path) && isLong(change.before) }">{{ formatValue(change.before) }}</p>
            </div>
            <ArrowRight :size="15" aria-hidden="true" />
            <div class="revision-diff__value revision-diff__value--after">
              <span>修改后</span>
              <p :class="{ 'is-clamped': !expanded.has(change.path) && isLong(change.after) }">{{ formatValue(change.after) }}</p>
            </div>
          </div>
          <button
            v-if="isLong(change.before) || isLong(change.after)"
            class="button button--quiet"
            type="button"
            :aria-expanded="expanded.has(change.path)"
            @click="toggle(change.path)"
          >
            <component :is="expanded.has(change.path) ? ChevronDown : ChevronRight" :size="14" />
            {{ expanded.has(change.path) ? '收起完整值' : '展开完整值' }}
          </button>
        </li>
      </ul>
    </template>
  </section>
</template>

<style scoped>
.revision-diff { display: flex; flex-direction: column; gap: 0.5rem; padding-top: 0.7rem; margin-top: 0.6rem; border-top: 1px solid var(--admin-line-soft); }
.revision-diff__header { display: flex; align-items: center; justify-content: space-between; }
.revision-diff__header h3 { margin: 0; font-size: 0.72rem; }
.revision-diff__count { color: var(--admin-muted); font-size: 0.58rem; }
.revision-diff__state { display: flex; align-items: center; gap: 0.35rem; margin: 0; color: var(--admin-muted); font-size: 0.62rem; }
.revision-diff__state--error { color: #b42318; }
.revision-diff__filter { margin: 0; }
.revision-diff__list { display: flex; flex-direction: column; gap: 0.5rem; margin: 0; padding: 0; list-style: none; }
.revision-diff__list > li { display: flex; flex-direction: column; gap: 0.3rem; padding: 0.5rem; border: 1px solid var(--admin-line); border-radius: 8px; background: white; }
.revision-diff__path { margin: 0; }
.revision-diff__path code { color: var(--airtek-blue-dark); font-size: 0.58rem; overflow-wrap: anywhere; }
.revision-diff__values { display: grid; grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr); align-items: center; gap: 0.35rem; }
.revision-diff__values > svg { color: var(--admin-muted); }
.revision-diff__value { min-width: 0; padding: 0.4rem 0.5rem; border-radius: 6px; }
.revision-diff__value span { display: block; margin-bottom: 0.15rem; color: var(--admin-muted); font-size: 0.52rem; letter-spacing: 0.05em; text-transform: uppercase; }
.revision-diff__value p { margin: 0; font-size: 0.6rem; line-height: 1.5; overflow-wrap: anywhere; white-space: pre-wrap; }
.revision-diff__value p.is-clamped { display: -webkit-box; overflow: hidden; -webkit-box-orient: vertical; -webkit-line-clamp: 3; }
.revision-diff__value--before { background: var(--admin-soft-red); }
.revision-diff__value--after { background: var(--admin-soft-green); }
</style>
