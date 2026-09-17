<script setup lang="ts">
import { ArrowDown, ArrowUp, Plus, Trash2 } from 'lucide-vue-next'
import type { FooterColumn, FooterTypeFields, NavigationItem } from '@airtek/contracts'
import { newDraftId } from '@/services/contentDraftDefaults'
import NavigationItemEditor from '../fields/NavigationItemEditor.vue'

const props = defineProps<{ modelValue: FooterTypeFields }>()
const emit = defineEmits<{ 'update:modelValue': [value: FooterTypeFields] }>()

function setColumns(columns: FooterColumn[]): void {
  emit('update:modelValue', { ...props.modelValue, columns })
}

function updateColumn(index: number, values: Partial<FooterColumn>): void {
  setColumns(props.modelValue.columns.map((column, position) => (
    position === index ? { ...column, ...values } : column
  )))
}

function addColumn(): void {
  setColumns([...props.modelValue.columns, { id: newDraftId(), title: '', links: [] }])
}

function removeColumn(index: number): void {
  setColumns(props.modelValue.columns.filter((_, position) => position !== index))
}

function moveColumn(index: number, direction: 'up' | 'down'): void {
  const target = direction === 'up' ? index - 1 : index + 1
  if (target < 0 || target >= props.modelValue.columns.length) return
  const columns = [...props.modelValue.columns]
  const [entry] = columns.splice(index, 1)
  columns.splice(target, 0, entry)
  setColumns(columns)
}

function updateColumnLink(columnIndex: number, linkIndex: number, link: NavigationItem): void {
  const column = props.modelValue.columns[columnIndex]
  updateColumn(columnIndex, {
    links: column.links.map((entry, position) => (position === linkIndex ? link : entry)),
  })
}

function addColumnLink(columnIndex: number): void {
  const column = props.modelValue.columns[columnIndex]
  updateColumn(columnIndex, {
    links: [...column.links, { id: newDraftId(), label: '', target: null, children: [] }],
  })
}

function removeColumnLink(columnIndex: number, linkIndex: number): void {
  const column = props.modelValue.columns[columnIndex]
  updateColumn(columnIndex, { links: column.links.filter((_, position) => position !== linkIndex) })
}

function moveColumnLink(columnIndex: number, linkIndex: number, direction: 'up' | 'down'): void {
  const column = props.modelValue.columns[columnIndex]
  const target = direction === 'up' ? linkIndex - 1 : linkIndex + 1
  if (target < 0 || target >= column.links.length) return
  const links = [...column.links]
  const [entry] = links.splice(linkIndex, 1)
  links.splice(target, 0, entry)
  updateColumn(columnIndex, { links })
}

function setLegalLinks(legalLinks: NavigationItem[]): void {
  emit('update:modelValue', { ...props.modelValue, legalLinks })
}

function updateLegalLink(index: number, link: NavigationItem): void {
  setLegalLinks(props.modelValue.legalLinks.map((entry, position) => (position === index ? link : entry)))
}

function removeLegalLink(index: number): void {
  setLegalLinks(props.modelValue.legalLinks.filter((_, position) => position !== index))
}

function moveLegalLink(index: number, direction: 'up' | 'down'): void {
  const target = direction === 'up' ? index - 1 : index + 1
  if (target < 0 || target >= props.modelValue.legalLinks.length) return
  const legalLinks = [...props.modelValue.legalLinks]
  const [entry] = legalLinks.splice(index, 1)
  legalLinks.splice(target, 0, entry)
  setLegalLinks(legalLinks)
}

function addLegalLink(): void {
  setLegalLinks([...props.modelValue.legalLinks, { id: newDraftId(), label: '', target: null, children: [] }])
}
</script>

<template>
  <div class="footer-fields">
    <section class="footer-fields__group" aria-labelledby="footer-columns">
      <header>
        <h3 id="footer-columns">页脚栏目</h3>
        <button class="button button--quiet" type="button" @click="addColumn"><Plus :size="14" />添加栏目</button>
      </header>
      <p v-if="!modelValue.columns.length" class="empty-mini">还没有页脚栏目。</p>
      <article v-for="(column, index) in modelValue.columns" :key="column.id" class="footer-fields__card">
        <div class="footer-fields__head">
          <label class="field">
            <span>栏目标题</span>
            <input
              :value="column.title"
              maxlength="120"
              :aria-label="`栏目 ${index + 1} 标题`"
              @input="updateColumn(index, { title: ($event.target as HTMLInputElement).value })"
            />
          </label>
          <div class="footer-fields__controls">
            <button class="icon-button" type="button" :disabled="index === 0" :aria-label="`上移栏目 ${index + 1}`" @click="moveColumn(index, 'up')"><ArrowUp :size="15" /></button>
            <button class="icon-button" type="button" :disabled="index === modelValue.columns.length - 1" :aria-label="`下移栏目 ${index + 1}`" @click="moveColumn(index, 'down')"><ArrowDown :size="15" /></button>
            <button class="icon-button" type="button" :aria-label="`删除栏目 ${index + 1}`" @click="removeColumn(index)"><Trash2 :size="15" /></button>
          </div>
        </div>
        <ul class="footer-fields__links">
          <NavigationItemEditor
            v-for="(link, linkIndex) in column.links"
            :key="link.id"
            :model-value="link"
            :index="linkIndex"
            :count="column.links.length"
            @update:model-value="updateColumnLink(index, linkIndex, $event)"
            @remove="removeColumnLink(index, linkIndex)"
            @move="moveColumnLink(index, linkIndex, $event)"
          />
        </ul>
        <button class="button button--quiet" type="button" @click="addColumnLink(index)"><Plus :size="14" />添加链接</button>
      </article>
    </section>

    <section class="footer-fields__group" aria-labelledby="footer-legal">
      <header>
        <h3 id="footer-legal">法律链接</h3>
        <button class="button button--quiet" type="button" @click="addLegalLink"><Plus :size="14" />添加法律链接</button>
      </header>
      <p v-if="!modelValue.legalLinks.length" class="empty-mini">还没有法律链接。</p>
      <ul class="footer-fields__links">
        <NavigationItemEditor
          v-for="(link, index) in modelValue.legalLinks"
          :key="link.id"
          :model-value="link"
          :index="index"
          :count="modelValue.legalLinks.length"
          @update:model-value="updateLegalLink(index, $event)"
          @remove="removeLegalLink(index)"
          @move="moveLegalLink(index, $event)"
        />
      </ul>
    </section>
  </div>
</template>

<style scoped>
@layer components {
.footer-fields { display: flex; flex-direction: column; gap: 1rem; }
.footer-fields__group { display: flex; flex-direction: column; gap: 0.6rem; }
.footer-fields__group header { display: flex; align-items: center; justify-content: space-between; }
.footer-fields__group h3 { margin: 0; color: var(--airtek-blue-dark); font-size: 0.75rem; letter-spacing: 0.06em; text-transform: uppercase; }
.footer-fields__card { display: flex; flex-direction: column; gap: 0.5rem; padding: 0.6rem; border: 1px solid var(--border-default); border-radius: 9px; background: white; }
.footer-fields__head { display: flex; align-items: flex-end; gap: 0.4rem; }
.footer-fields__head .field { flex: 1; }
.footer-fields__controls { display: flex; align-items: center; gap: 0.15rem; }
.footer-fields__links { display: flex; flex-direction: column; gap: 0.2rem; margin: 0; padding: 0; }
}
</style>
