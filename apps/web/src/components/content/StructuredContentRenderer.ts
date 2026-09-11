import { Fragment, defineComponent, h, type PropType, type VNodeChild } from 'vue'
import type { TiptapDocument } from '@airtek/contracts'
import {
  asRichTextNode,
  richTextPlainText,
  safeImageUrl,
  safeLinkUrl,
  type RichTextMark,
  type RichTextNode,
} from '@/lib/richText'
import { articleOutlineHeadingTitle, extractArticleOutline } from '@/lib/articlePresentation'

const maximumDepth = 32
const maximumNodes = 2_000
const entityKinds = new Set(['media', 'cta', 'related-product', 'related-content', 'formula'])

interface RenderState {
  count: number
  headingIds: string[]
  headingIndex: number
}

function textAttribute(value: unknown, fallback = ''): string {
  return typeof value === 'string' ? value.slice(0, 1_000) : fallback
}

function numericAttribute(value: unknown): number | undefined {
  const number = typeof value === 'number' ? value : Number(value)
  return Number.isFinite(number) && number > 0 && number <= 8_192 ? Math.round(number) : undefined
}

function linkProperties(href: string, attrs?: Record<string, unknown>) {
  const external = href.startsWith('https://')
  const newWindow = external && attrs?.target === '_blank'
  return {
    href,
    ...(newWindow ? { target: '_blank', rel: 'noopener noreferrer' } : {}),
  }
}

function renderMarks(text: string, marks: RichTextMark[] | undefined): VNodeChild {
  let rendered: VNodeChild = text
  for (const mark of marks ?? []) {
    if (mark.type === 'bold') rendered = h('strong', [rendered])
    else if (mark.type === 'italic') rendered = h('em', [rendered])
    else if (mark.type === 'strike') rendered = h('s', [rendered])
    else if (mark.type === 'underline') rendered = h('u', [rendered])
    else if (mark.type === 'code') rendered = h('code', { class: 'rich-content__inline-code' }, [rendered])
    else if (mark.type === 'link') {
      const href = safeLinkUrl(mark.attrs?.href)
      if (href) rendered = h('a', linkProperties(href, mark.attrs), [rendered])
    }
  }
  return rendered
}

function renderChildren(node: RichTextNode, state: RenderState, depth: number): VNodeChild[] {
  return (node.content ?? []).flatMap((child, index) => {
    const rendered = renderNode(child, state, depth + 1, index)
    return rendered == null ? [] : [rendered]
  })
}

function renderImage(node: RichTextNode, key: number): VNodeChild | null {
  const src = safeImageUrl(node.attrs?.src)
  if (!src) return null
  const alt = textAttribute(node.attrs?.alt)
  const caption = textAttribute(node.attrs?.caption)
  const image = h('img', {
    src,
    alt,
    width: numericAttribute(node.attrs?.width),
    height: numericAttribute(node.attrs?.height),
    loading: 'lazy',
    decoding: 'async',
    referrerpolicy: 'strict-origin-when-cross-origin',
  })
  return h('figure', { class: 'rich-content__media', key }, [image, ...(caption ? [h('figcaption', caption)] : [])])
}

function renderCallToAction(node: RichTextNode, key: number): VNodeChild | null {
  const href = safeLinkUrl(node.attrs?.href)
  const label = textAttribute(node.attrs?.label) || richTextPlainText(node).trim()
  const description = textAttribute(node.attrs?.description)
  if (!label) return null
  if (!href) {
    return h('aside', { class: 'rich-content__callout', role: 'note', key }, [
      h('strong', label),
      ...(description ? [h('p', description)] : []),
    ])
  }
  return h('div', { class: 'rich-content__cta', key }, [
    ...(description ? [h('p', description)] : []),
    h('a', { ...linkProperties(href, node.attrs), class: 'button' }, label),
  ])
}

function renderEntityBlock(node: RichTextNode, key: number): VNodeChild | null {
  const kind = textAttribute(node.attrs?.kind)
  if (!entityKinds.has(kind)) return null
  if (kind === 'media') return renderImage({ ...node, attrs: { ...node.attrs, src: node.attrs?.src } }, key)
  if (kind === 'cta') return renderCallToAction(node, key)
  if (kind === 'formula') {
    const expression = textAttribute(node.attrs?.expression) || textAttribute(node.attrs?.reference) || richTextPlainText(node)
    if (!expression.trim()) return null
    return h('div', { class: 'rich-content__math', role: 'figure', 'aria-label': 'Formula', key }, [h('code', expression)])
  }

  const label = textAttribute(node.attrs?.label, kind === 'related-product' ? 'Related product' : 'Related content')
  const reference = textAttribute(node.attrs?.reference)
  const href = safeLinkUrl(node.attrs?.href)
  const content = [h('span', { class: 'eyebrow' }, label), h('strong', reference || 'Published relationship')]
  return href
    ? h('a', { ...linkProperties(href, node.attrs), class: 'rich-content__entity', key }, content)
    : h('aside', { class: 'rich-content__entity', 'aria-label': label, key }, content)
}

function renderNode(node: RichTextNode, state: RenderState, depth: number, key: number): VNodeChild | null {
  state.count += 1
  if (depth > maximumDepth || state.count > maximumNodes) return null
  if (node.type === 'text') return renderMarks(node.text ?? '', node.marks)
  if (node.type === 'hardBreak') return h('br', { key })

  const children = () => renderChildren(node, state, depth)
  if (node.type === 'paragraph') return h('p', { key }, children())
  if (node.type === 'heading') {
    const requested = Number(node.attrs?.level)
    const level = requested === 3 || requested === 4 ? requested : 2
    const hasOutlineTitle = Boolean(articleOutlineHeadingTitle(node))
    const id = hasOutlineTitle ? state.headingIds[state.headingIndex++] : undefined
    return h(`h${level}`, { ...(id ? { id } : {}), key }, children())
  }
  if (node.type === 'bulletList') return h('ul', { key }, children())
  if (node.type === 'orderedList') return h('ol', { key }, children())
  if (node.type === 'listItem') return h('li', { key }, children())
  if (node.type === 'blockquote') return h('blockquote', { key }, children())
  if (node.type === 'bold') return h('strong', { key }, children())
  if (node.type === 'italic') return h('em', { key }, children())
  if (node.type === 'strike') return h('s', { key }, children())
  if (node.type === 'underline') return h('u', { key }, children())
  if (node.type === 'code') return h('code', { class: 'rich-content__inline-code', key }, children())
  if (node.type === 'link') {
    const href = safeLinkUrl(node.attrs?.href)
    return href ? h('a', { ...linkProperties(href, node.attrs), key }, children()) : h(Fragment, { key }, children())
  }
  if (node.type === 'image') return renderImage(node, key)
  if (node.type === 'gallery') return h('div', { class: 'rich-content__gallery', role: 'group', 'aria-label': textAttribute(node.attrs?.label, 'Image gallery'), key }, children())
  if (node.type === 'table') {
    const caption = textAttribute(node.attrs?.caption)
    const label = textAttribute(node.attrs?.label, 'Published data table')
    return h('div', { class: 'rich-content__table-wrap', key }, [
      h('table', { ...(caption ? {} : { 'aria-label': label }) }, [
        ...(caption ? [h('caption', caption)] : []),
        h('tbody', children()),
      ]),
    ])
  }
  if (node.type === 'tableRow') return h('tr', { key }, children())
  if (node.type === 'tableHeader') return h('th', { scope: 'col', key }, children())
  if (node.type === 'tableCell') return h('td', { key }, children())
  if (node.type === 'codeBlock') {
    const language = textAttribute(node.attrs?.language).toLowerCase()
    const safeLanguage = /^[a-z0-9+#.-]{1,24}$/.test(language) ? language : ''
    return h('pre', { class: 'rich-content__code', key }, [
      h('code', { ...(safeLanguage ? { class: `language-${safeLanguage}` } : {}), 'aria-label': safeLanguage ? `${safeLanguage} code` : 'Code' }, richTextPlainText(node)),
    ])
  }
  if (node.type === 'mathBlock') {
    const expression = textAttribute(node.attrs?.expression) || richTextPlainText(node)
    return expression.trim()
      ? h('div', { class: 'rich-content__math', role: 'figure', 'aria-label': textAttribute(node.attrs?.label, 'Formula'), key }, [h('code', expression)])
      : null
  }
  if (node.type === 'callout') {
    const label = textAttribute(node.attrs?.label, 'Note')
    return h('aside', { class: 'rich-content__callout', role: 'note', 'aria-label': label, key }, [h('strong', label), ...children()])
  }
  if (node.type === 'cta') return renderCallToAction(node, key)
  if (node.type === 'relatedContent') {
    const href = safeLinkUrl(node.attrs?.href)
    const label = textAttribute(node.attrs?.label, 'Related content')
    return h('aside', { class: 'rich-content__related', 'aria-label': label, key }, [
      h('strong', label),
      ...(href ? [h('a', linkProperties(href, node.attrs), richTextPlainText(node) || 'Open related content')] : children()),
    ])
  }
  if (node.type === 'entityBlock') return renderEntityBlock(node, key)
  return null
}

export default defineComponent({
  name: 'StructuredContentRenderer',
  props: {
    document: { type: Object as PropType<TiptapDocument>, required: true },
  },
  setup(props) {
    return () => {
      const root = asRichTextNode(props.document)
      if (!root || root.type !== 'doc') return null
      const state: RenderState = {
        count: 0,
        headingIds: extractArticleOutline(props.document).map((item) => item.id),
        headingIndex: 0,
      }
      return h('article', { class: 'rich-content prose', 'data-schema-version': '2' }, renderChildren(root, state, 0))
    }
  },
})
