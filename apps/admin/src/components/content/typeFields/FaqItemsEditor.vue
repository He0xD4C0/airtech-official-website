<script setup lang="ts">
import { ArrowDown, ArrowUp, Plus, Trash2 } from 'lucide-vue-next'
import type { FaqItem } from '@airtek/contracts'
import { newDraftId } from '@/services/contentDraftDefaults'
import { documentToPlainText, plainTextToDocument } from './tiptapPlainText'

const props = defineProps<{ modelValue: FaqItem[] }>()
const emit = defineEmits<{ 'update:modelValue': [value: FaqItem[]] }>()

function update(index: number, next: FaqItem): void {
  emit('update:modelValue', props.modelValue.map((item, position) => (position === index ? next : item)))
}

function updateQuestion(index: number, question: string): void {
  update(index, { ...props.modelValue[index], question })
}

function updateAnswer(index: number, answer: string): void {
  update(index, { ...props.modelValue[index], answer: plainTextToDocument(answer) })
}

function remove(index: number): void {
  emit('update:modelValue', props.modelValue.filter((_, position) => position !== index))
}

function move(index: number, direction: 'up' | 'down'): void {
  const target = direction === 'up' ? index - 1 : index + 1
  if (target < 0 || target >= props.modelValue.length) return
  const next = [...props.modelValue]
  const [entry] = next.splice(index, 1)
  next.splice(target, 0, entry)
  emit('update:modelValue', next)
}

function add(): void {
  emit('update:modelValue', [...props.modelValue, {
    id: newDraftId(),
    question: '',
    answer: plainTextToDocument(''),
  }])
}
</script>

<template>
  <section class="faq-items" aria-labelledby="faq-items-title">
    <header>
      <strong id="faq-items-title">FAQ 条目（{{ modelValue.length }}）</strong>
      <button class="button button--quiet" type="button" @click="add"><Plus :size="14" />添加条目</button>
    </header>
    <p v-if="!modelValue.length" class="empty-mini">还没有 FAQ 条目。</p>
    <ol class="faq-items__list">
      <li v-for="(item, index) in modelValue" :key="item.id">
        <div class="faq-items__head">
          <strong>条目 {{ index + 1 }}</strong>
          <div>
            <button class="icon-button" type="button" :disabled="index === 0" :aria-label="`上移条目 ${index + 1}`" @click="move(index, 'up')">
              <ArrowUp :size="14" />
            </button>
            <button class="icon-button" type="button" :disabled="index === modelValue.length - 1" :aria-label="`下移条目 ${index + 1}`" @click="move(index, 'down')">
              <ArrowDown :size="14" />
            </button>
            <button class="icon-button" type="button" :aria-label="`删除条目 ${index + 1}`" @click="remove(index)">
              <Trash2 :size="14" />
            </button>
          </div>
        </div>
        <label class="field">
          <span>问题</span>
          <input
            :value="item.question"
            maxlength="300"
            placeholder="例如 What voltages are available?"
            @input="updateQuestion(index, ($event.target as HTMLInputElement).value)"
          />
        </label>
        <label class="field">
          <span>答案<small>在此处保存会以纯文本重建答案文档（原富文本格式会被替换）</small></span>
          <textarea
            :value="documentToPlainText(item.answer)"
            rows="3"
            @input="updateAnswer(index, ($event.target as HTMLTextAreaElement).value)"
          />
        </label>
      </li>
    </ol>
  </section>
</template>

<style scoped>
.faq-items { display: flex; flex-direction: column; gap: 0.5rem; }
.faq-items header { display: flex; align-items: center; justify-content: space-between; }
.faq-items header strong { font-size: 0.64rem; }
.faq-items__list { display: flex; flex-direction: column; gap: 0.5rem; margin: 0; padding: 0; list-style: none; }
.faq-items__list > li { display: flex; flex-direction: column; gap: 0.45rem; padding: 0.6rem; border: 1px solid var(--admin-line); border-radius: 9px; background: white; }
.faq-items__head { display: flex; align-items: center; justify-content: space-between; }
.faq-items__head strong { color: var(--admin-muted); font-size: 0.58rem; letter-spacing: 0.05em; text-transform: uppercase; }
.faq-items__head > div { display: flex; gap: 0.15rem; }
</style>
