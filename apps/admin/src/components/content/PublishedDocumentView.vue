<script setup lang="ts">
import { computed } from 'vue'
import type { ContentDraftV2 } from '@airtek/contracts'

const props = defineProps<{ document: ContentDraftV2 }>()

interface SummaryRow { label: string; value: string }

function label(path: string): string {
  return path.replace(/([a-z])([A-Z])/g, '$1 $2').replaceAll('.', ' / ')
}

function scalar(value: unknown): string | null {
  if (typeof value === 'string') return value || '—'
  if (typeof value === 'number' || typeof value === 'boolean') return String(value)
  return value === null ? '—' : null
}

function flatten(value: unknown, prefix = ''): SummaryRow[] {
  const direct = scalar(value)
  if (direct !== null) return [{ label: label(prefix), value: direct }]
  if (Array.isArray(value)) {
    if (!value.length) return [{ label: label(prefix), value: '无' }]
    if (value.every((entry) => scalar(entry) !== null)) {
      return [{ label: label(prefix), value: value.map((entry) => scalar(entry)).join('、') }]
    }
    return [{ label: label(prefix), value: `${value.length} 项` }]
  }
  if (!value || typeof value !== 'object') return []
  return Object.entries(value).flatMap(([key, entry]) => flatten(entry, prefix ? `${prefix}.${key}` : key))
}

function bodyText(node: unknown): string[] {
  if (!node || typeof node !== 'object') return []
  const record = node as { text?: unknown; content?: unknown }
  const own = typeof record.text === 'string' ? [record.text] : []
  const children = Array.isArray(record.content) ? record.content.flatMap(bodyText) : []
  return [...own, ...children]
}

const typeRows = computed(() => flatten(props.document.typeFields))
const bodyParagraphs = computed(() => bodyText(props.document.body).filter((entry) => entry.trim()))
</script>

<template>
  <div class="published-document">
    <section class="panel published-document__section">
      <h2>文档信息</h2>
      <dl class="published-document__grid">
        <div><dt>内容类型</dt><dd>{{ document.kind }}</dd></div>
        <div><dt>模板</dt><dd>{{ document.templateKey }}</dd></div>
        <div><dt>Locale</dt><dd>{{ document.locale }}</dd></div>
        <div><dt>Slug</dt><dd>{{ document.slug || '不产生路由' }}</dd></div>
        <div><dt>占位内容</dt><dd>{{ document.isPlaceholder ? '是（noindex）' : '否' }}</dd></div>
        <div><dt>文档版本</dt><dd>{{ document.draftVersion }}</dd></div>
      </dl>
      <p v-if="document.summary" class="published-document__summary">{{ document.summary }}</p>
    </section>

    <section class="panel published-document__section">
      <h2>页面区块</h2>
      <p v-if="!document.composition.blocks.length" class="published-document__empty">没有页面区块。</p>
      <ol v-else class="published-document__blocks">
        <li v-for="block in document.composition.blocks" :key="block.id">
          <strong>{{ block.type }}</strong>
          <span>{{ flatten(block).filter((row) => !['id', 'type'].includes(row.label)).slice(0, 4).map((row) => `${row.label}: ${row.value}`).join(' · ') || '无附加字段' }}</span>
        </li>
      </ol>
    </section>

    <section v-if="document.body" class="panel published-document__section">
      <h2>正文</h2>
      <div v-if="bodyParagraphs.length" class="published-document__body">
        <p v-for="(paragraph, index) in bodyParagraphs" :key="index">{{ paragraph }}</p>
      </div>
      <p v-else class="published-document__empty">正文包含结构节点，但没有可读文本。</p>
    </section>

    <section class="panel published-document__section">
      <h2>类型字段</h2>
      <dl v-if="typeRows.length" class="published-document__rows">
        <div v-for="row in typeRows" :key="row.label"><dt>{{ row.label }}</dt><dd>{{ row.value }}</dd></div>
      </dl>
      <p v-else class="published-document__empty">没有类型专属字段。</p>
    </section>

    <section class="panel published-document__section">
      <h2>关系与 SEO</h2>
      <dl class="published-document__rows">
        <div><dt>显式关系</dt><dd>{{ document.relations.length }} 条</dd></div>
        <div><dt>SEO 标题</dt><dd>{{ document.seo.title || '使用页面标题' }}</dd></div>
        <div><dt>SEO 描述</dt><dd>{{ document.seo.description || '未单独设置' }}</dd></div>
        <div><dt>允许索引</dt><dd>{{ document.seo.indexable ? '是' : '否' }}</dd></div>
        <div><dt>社交图片</dt><dd>{{ document.seo.socialImage ? '已选择媒体资产' : '未设置' }}</dd></div>
      </dl>
    </section>

    <details class="panel published-document__diagnostics">
      <summary>开发诊断：查看原始 JSON</summary>
      <pre>{{ JSON.stringify(document, null, 2) }}</pre>
    </details>
  </div>
</template>

<style scoped>
.published-document { display: grid; gap: 1rem; }
.published-document__section { display: flex; flex-direction: column; gap: .75rem; }
.published-document h2 { margin: 0; font-size: 1rem; }
.published-document__grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(11rem, 1fr)); gap: .65rem; margin: 0; }
.published-document__grid div, .published-document__rows div { padding: .65rem; border: 1px solid var(--admin-line); border-radius: 8px; }
.published-document dt { color: var(--admin-muted); font-size: .75rem; }
.published-document dd { margin: .2rem 0 0; overflow-wrap: anywhere; }
.published-document__summary, .published-document__body p, .published-document__empty { margin: 0; line-height: 1.65; }
.published-document__blocks { display: flex; flex-direction: column; gap: .45rem; margin: 0; padding-left: 1.3rem; }
.published-document__blocks li { padding: .5rem; }
.published-document__blocks strong, .published-document__blocks span { display: block; }
.published-document__blocks span { color: var(--admin-muted); font-size: .75rem; }
.published-document__rows { display: grid; gap: .45rem; margin: 0; }
.published-document__diagnostics summary { cursor: pointer; font-weight: 700; }
.published-document__diagnostics pre { max-height: 50vh; overflow: auto; white-space: pre-wrap; }
</style>
