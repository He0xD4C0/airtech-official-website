import { createSSRApp, h } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import type { TiptapDocument } from '@airtek/contracts'
import StructuredContentRenderer from '@/features/content/components/content/StructuredContentRenderer'
import { safeImageUrl, safeLinkUrl } from '@/shared/lib/richText'

function document(content: unknown[]): TiptapDocument {
  return { type: 'doc', content } as TiptapDocument
}

async function render(documentValue: TiptapDocument): Promise<string> {
  return renderToString(createSSRApp({
    render: () => h(StructuredContentRenderer, { document: documentValue }),
  }))
}

describe('structured public content renderer', () => {
  it('SSR-renders the versioned allowlist without raw HTML', async () => {
    const html = await render(document([
      { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'Published heading' }] },
      { type: 'paragraph', content: [
        { type: 'text', text: '<script>alert(1)</script>', marks: [{ type: 'bold' }] },
        { type: 'text', text: ' internal link', marks: [{ type: 'link', attrs: { href: '/en/products' } }] },
      ] },
      { type: 'blockquote', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Evidence first' }] }] },
      { type: 'image', attrs: { src: 'https://cdn.example.com/fan.webp', alt: 'Validated fan assembly', caption: 'Approved media' } },
      { type: 'table', attrs: { caption: 'Operating context' }, content: [
        { type: 'tableRow', content: [
          { type: 'tableHeader', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Field' }] }] },
          { type: 'tableCell', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Value' }] }] },
        ] },
      ] },
      { type: 'codeBlock', attrs: { language: 'ts' }, content: [{ type: 'text', text: 'const safe = true' }] },
      { type: 'mathBlock', attrs: { expression: 'Q = vA', label: 'Airflow formula' } },
      { type: 'callout', attrs: { label: 'Engineering note' }, content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Check units.' }] }] },
      { type: 'cta', attrs: { href: '/en/request-a-quote', label: 'Request a quote' } },
      { type: 'entityBlock', attrs: { kind: 'related-content', label: 'Related topic', reference: 'Airflow and pressure', href: '/en/technology/airflow-and-pressure' } },
    ]))

    expect(html).toContain('<h2 id="published-heading">Published heading</h2>')
    expect(html).toContain('&lt;script&gt;alert(1)&lt;/script&gt;')
    expect(html).not.toContain('<script>')
    expect(html).toContain('href="/en/products"')
    expect(html).toContain('alt="Validated fan assembly"')
    expect(html).toContain('<caption>Operating context</caption>')
    expect(html).toContain('aria-label="Airflow formula"')
    expect(html).toContain('role="note"')
    expect(html).toContain('Request a quote')
    expect(html).toContain('Airflow and pressure')
  })

  it('drops unknown nodes and unsafe URLs while preserving safe text', async () => {
    const html = await render(document([
      { type: 'paragraph', content: [
        { type: 'text', text: 'unsafe destination', marks: [{ type: 'link', attrs: { href: 'javascript:alert(1)' } }] },
      ] },
      { type: 'image', attrs: { src: 'data:image/svg+xml,<svg onload=alert(1)>', alt: 'unsafe' } },
      { type: 'iframe', attrs: { src: 'https://example.com' }, content: [{ type: 'text', text: 'must disappear' }] },
      { type: 'entityBlock', attrs: { kind: 'arbitrary-html', reference: '<img src=x onerror=alert(1)>' } },
    ]))

    expect(html).toContain('unsafe destination')
    expect(html).not.toContain('javascript:')
    expect(html).not.toContain('<img')
    expect(html).not.toContain('iframe')
    expect(html).not.toContain('must disappear')
    expect(html).not.toContain('onerror')
  })

  it('accepts only explicit navigation and media protocols', () => {
    expect(safeLinkUrl('/en/solutions?source=article#context')).toBe('/en/solutions?source=article#context')
    expect(safeLinkUrl('https://example.com/resource')).toBe('https://example.com/resource')
    expect(safeLinkUrl('mailto:engineering@example.com')).toBe('mailto:engineering@example.com')
    expect(safeLinkUrl('http://example.com')).toBeUndefined()
    expect(safeLinkUrl('//example.com')).toBeUndefined()
    expect(safeLinkUrl('javascript:alert(1)')).toBeUndefined()
    expect(safeImageUrl('data:image/png;base64,AAAA')).toBeUndefined()
  })

  it('marks the rendered native document as schema version 2', async () => {
    const html = await render(document([{ type: 'paragraph', content: [{ type: 'text', text: 'Published' }] }]))
    expect(html).toContain('data-schema-version="2"')
  })
})
