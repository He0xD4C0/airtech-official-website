<script setup lang="ts">
import { onBeforeUnmount, watch } from 'vue'
import { EditorContent, useEditor } from '@tiptap/vue-3'
import { mergeAttributes, Node } from '@tiptap/core'
import StarterKit from '@tiptap/starter-kit'
import Link from '@tiptap/extension-link'
import Placeholder from '@tiptap/extension-placeholder'
import Table from '@tiptap/extension-table'
import TableRow from '@tiptap/extension-table-row'
import TableHeader from '@tiptap/extension-table-header'
import TableCell from '@tiptap/extension-table-cell'
import {
  Bold,
  Braces,
  ChevronDown,
  Code2,
  Heading2,
  Heading3,
  Heading4,
  Image,
  Italic,
  Link2,
  List,
  ListOrdered,
  MessageSquareQuote,
  Redo2,
  RemoveFormatting,
  Table2,
  Undo2,
} from 'lucide-vue-next'

interface EditorDocument {
  type: 'doc'
  attrs?: Record<string, unknown>
  content?: Array<Record<string, unknown>>
  schemaVersion?: number
}

const props = defineProps<{ modelValue: EditorDocument }>()
const emit = defineEmits<{ 'update:modelValue': [value: EditorDocument] }>()

const EntityBlock = Node.create({
  name: 'entityBlock',
  group: 'block',
  atom: true,
  draggable: true,
  addAttributes() {
    return {
      kind: { default: 'related-product' },
      label: { default: '关联内容' },
      reference: { default: '尚未选择' },
    }
  },
  parseHTML() {
    return [{ tag: 'div[data-airtek-block]' }]
  },
  renderHTML({ HTMLAttributes }) {
    return [
      'div',
      mergeAttributes(HTMLAttributes, {
        'data-airtek-block': HTMLAttributes.kind,
        class: 'editor-entity-block',
      }),
      ['span', { class: 'editor-entity-block__kind' }, String(HTMLAttributes.label)],
      ['strong', {}, String(HTMLAttributes.reference)],
      ['small', {}, '点击右侧“关系”面板完成配置'],
    ]
  },
})

const editor = useEditor({
  content: props.modelValue,
  extensions: [
    StarterKit.configure({ heading: { levels: [2, 3, 4] } }),
    Link.configure({ openOnClick: false, protocols: ['https', 'mailto'] }),
    Placeholder.configure({ placeholder: '开始编写内容，或从左侧插入结构化区块…' }),
    Table.configure({ resizable: true }),
    TableRow,
    TableHeader,
    TableCell,
    EntityBlock,
  ],
  editorProps: {
    attributes: {
      class: 'structured-editor__content',
      role: 'textbox',
      'aria-label': '内容正文编辑器',
      'aria-multiline': 'true',
    },
  },
  onUpdate: ({ editor: currentEditor }) => {
    emit('update:modelValue', {
      ...(currentEditor.getJSON() as EditorDocument),
      ...(props.modelValue.attrs ? { attrs: props.modelValue.attrs } : {}),
      schemaVersion: 1,
    })
  },
})

watch(() => props.modelValue, (value) => {
  if (!editor.value) return
  const current = JSON.stringify(editor.value.getJSON())
  const incoming = JSON.stringify({ type: value.type, content: value.content })
  if (current !== incoming) editor.value.commands.setContent(value, false)
}, { deep: true })

function addLink(): void {
  const previous = editor.value?.getAttributes('link').href as string | undefined
  const href = window.prompt('输入 https:// 或 mailto: 链接', previous ?? 'https://')
  if (!href || !/^(https:\/\/|mailto:)/i.test(href)) return
  editor.value?.chain().focus().extendMarkRange('link').setLink({ href }).run()
}

function addTable(): void {
  editor.value?.chain().focus().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run()
}

function addEntityBlock(kind: string, label: string): void {
  editor.value?.chain().focus().insertContent({ type: 'entityBlock', attrs: { kind, label, reference: '尚未选择' } }).run()
}

onBeforeUnmount(() => editor.value?.destroy())
</script>

<template>
  <section class="structured-editor">
    <div v-if="editor" class="structured-editor__toolbar" role="toolbar" aria-label="文本格式">
      <div class="toolbar-group">
        <button type="button" :class="{ 'is-active': editor.isActive('bold') }" aria-label="粗体" @click="editor.chain().focus().toggleBold().run()"><Bold :size="16" /></button>
        <button type="button" :class="{ 'is-active': editor.isActive('italic') }" aria-label="斜体" @click="editor.chain().focus().toggleItalic().run()"><Italic :size="16" /></button>
        <button type="button" :class="{ 'is-active': editor.isActive('code') }" aria-label="行内代码" @click="editor.chain().focus().toggleCode().run()"><Code2 :size="16" /></button>
      </div>
      <div class="toolbar-group">
        <button type="button" :class="{ 'is-active': editor.isActive('heading', { level: 2 }) }" aria-label="二级标题" @click="editor.chain().focus().toggleHeading({ level: 2 }).run()"><Heading2 :size="17" /></button>
        <button type="button" :class="{ 'is-active': editor.isActive('heading', { level: 3 }) }" aria-label="三级标题" @click="editor.chain().focus().toggleHeading({ level: 3 }).run()"><Heading3 :size="17" /></button>
        <button type="button" :class="{ 'is-active': editor.isActive('heading', { level: 4 }) }" aria-label="四级标题" @click="editor.chain().focus().toggleHeading({ level: 4 }).run()"><Heading4 :size="17" /></button>
      </div>
      <div class="toolbar-group">
        <button type="button" :class="{ 'is-active': editor.isActive('bulletList') }" aria-label="无序列表" @click="editor.chain().focus().toggleBulletList().run()"><List :size="17" /></button>
        <button type="button" :class="{ 'is-active': editor.isActive('orderedList') }" aria-label="有序列表" @click="editor.chain().focus().toggleOrderedList().run()"><ListOrdered :size="17" /></button>
        <button type="button" :class="{ 'is-active': editor.isActive('blockquote') }" aria-label="引用" @click="editor.chain().focus().toggleBlockquote().run()"><MessageSquareQuote :size="17" /></button>
      </div>
      <div class="toolbar-group">
        <button type="button" :class="{ 'is-active': editor.isActive('link') }" aria-label="链接" @click="addLink"><Link2 :size="17" /></button>
        <button type="button" aria-label="插入表格" @click="addTable"><Table2 :size="17" /></button>
        <button type="button" aria-label="插入代码块" @click="editor.chain().focus().toggleCodeBlock().run()"><Braces :size="17" /></button>
        <button type="button" aria-label="清除格式" @click="editor.chain().focus().clearNodes().unsetAllMarks().run()"><RemoveFormatting :size="17" /></button>
      </div>
      <div class="toolbar-spacer"></div>
      <div class="toolbar-group">
        <button type="button" aria-label="撤销" :disabled="!editor.can().undo()" @click="editor.chain().focus().undo().run()"><Undo2 :size="17" /></button>
        <button type="button" aria-label="重做" :disabled="!editor.can().redo()" @click="editor.chain().focus().redo().run()"><Redo2 :size="17" /></button>
      </div>
    </div>

    <div class="structured-editor__body">
      <aside class="block-library" aria-label="内容区块">
        <p>内容区块</p>
        <button type="button" @click="addEntityBlock('media', '图片 / 图库')"><Image :size="17" /><span>图片 / 图库<small>从媒体库选择</small></span></button>
        <button type="button" @click="addEntityBlock('cta', '行动按钮')"><ChevronDown :size="17" /><span>CTA<small>行动按钮</small></span></button>
        <button type="button" @click="addEntityBlock('related-product', '关联产品')"><ChevronDown :size="17" /><span>关联产品<small>稳定产品 ID</small></span></button>
        <button type="button" @click="addEntityBlock('related-content', '关联内容')"><ChevronDown :size="17" /><span>关联内容<small>Solution / FAQ / Case</small></span></button>
        <button type="button" @click="addEntityBlock('formula', '公式')"><Braces :size="17" /><span>公式<small>LaTeX 表达式</small></span></button>
      </aside>
      <div class="structured-editor__canvas">
        <div class="paste-safety"><span>结构化 JSON</span> 粘贴 Word 或公众号内容时会按编辑器 Schema 清理，不保存原始 HTML。</div>
        <EditorContent :editor="editor" />
      </div>
    </div>
  </section>
</template>
