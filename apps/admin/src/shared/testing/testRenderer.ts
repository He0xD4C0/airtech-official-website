import { createRenderer, nextTick, type App, type Component, type RendererOptions } from 'vue'

interface TestTextNode {
  kind: 'text' | 'comment'
  value: string
  parent: TestElement | null
}

export interface TestElement {
  kind: 'element'
  type: string
  props: Record<string, unknown>
  children: TestNode[]
  parent: TestElement | null
}

export type TestNode = TestElement | TestTextNode

function element(type: string): TestElement {
  return { kind: 'element', type, props: {}, children: [], parent: null }
}

function textNode(kind: TestTextNode['kind'], value: string): TestTextNode {
  return { kind, value, parent: null }
}

function detach(node: TestNode): void {
  if (!node.parent) return
  const index = node.parent.children.indexOf(node)
  if (index >= 0) node.parent.children.splice(index, 1)
  node.parent = null
}

const rendererOptions: RendererOptions<TestNode, TestElement> = {
  patchProp(node, key, previousValue, nextValue) {
    void previousValue
    if (nextValue === null || nextValue === undefined) delete node.props[key]
    else node.props[key] = nextValue
  },
  insert(node, parent, anchor) {
    detach(node)
    const index = anchor ? parent.children.indexOf(anchor) : -1
    if (index >= 0) parent.children.splice(index, 0, node)
    else parent.children.push(node)
    node.parent = parent
  },
  remove: detach,
  createElement: element,
  createText: (value) => textNode('text', value),
  createComment: (value) => textNode('comment', value),
  setText(node, value) {
    if (node.kind === 'element') {
      const child = textNode('text', value)
      child.parent = node
      node.children = [child]
    } else node.value = value
  },
  setElementText(node, value) {
    for (const child of node.children) child.parent = null
    const child = textNode('text', value)
    child.parent = node
    node.children = [child]
  },
  parentNode: (node) => node.parent,
  nextSibling(node) {
    if (!node.parent) return null
    const index = node.parent.children.indexOf(node)
    return node.parent.children[index + 1] ?? null
  },
  querySelector: () => null,
  setScopeId(node, id) {
    node.props[id] = ''
  },
  insertStaticContent(content, parent, anchor) {
    const node = textNode('text', content)
    rendererOptions.insert(node, parent, anchor)
    return [node, node]
  },
}

const renderer = createRenderer<TestNode, TestElement>(rendererOptions)

export function mountForTest(
  component: Component,
  globalComponents: Record<string, Component> = {},
): { app: App<TestElement>; root: TestElement } {
  const root = element('test-root')
  const app = renderer.createApp(component)
  for (const [name, globalComponent] of Object.entries(globalComponents)) {
    app.component(name, globalComponent)
  }
  app.mount(root)
  return { app, root }
}

export function testTextContent(node: TestNode): string {
  if (node.kind !== 'element') return node.kind === 'text' ? node.value : ''
  return node.children.map(testTextContent).join('')
}

export function findTestElements(node: TestNode, type: string): TestElement[] {
  if (node.kind !== 'element') return []
  return [
    ...(node.type === type ? [node] : []),
    ...node.children.flatMap((child) => findTestElements(child, type)),
  ]
}

export async function flushTestUpdates(): Promise<void> {
  for (let index = 0; index < 5; index += 1) {
    await Promise.resolve()
    await nextTick()
  }
}
