import { describe, expect, it } from 'vitest'
import type { ContentEntry } from '@airtek/contracts'
import {
  extractPublishedDownloadMetadata,
  publishedDownloadAvailability,
  publishedDownloadListMetadata,
  safePublicDownloadUrl,
} from './downloadResources'
import type { PublicPageModel } from '@/types/content'

function downloadContent(attrs: Record<string, unknown>, options: { placeholder?: boolean; indexable?: boolean } = {}): ContentEntry {
  return {
    id: '64fe5b89-ab2a-4ac5-8c4e-44bf413e8a11', kind: 'download', slug: 'controlled-file', locale: 'en',
    title: 'Controlled file', summary: 'Published resource.',
    body: { schemaVersion: 1, doc: { type: 'doc', attrs, content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Resource context.' }] }] } },
    seo: { title: null, description: null, canonicalPath: '/en/resources/downloads/controlled-file', indexable: options.indexable ?? true },
    status: 'published', isPlaceholder: options.placeholder ?? false, currentRevision: 2, publishedRevision: 1,
    scheduledFor: null, updatedAt: '2026-09-01T08:00:00Z',
  }
}

function page(content: ContentEntry, overrides: Partial<PublicPageModel> = {}): PublicPageModel {
  return {
    kind: 'detail', collection: 'downloads', canonicalPath: '/en/resources/downloads/controlled-file',
    title: content.title, metaTitle: `${content.title} | AIRTEKPOWER`, description: content.summary ?? '',
    eyebrow: 'Controlled resource', breadcrumbs: [], indexable: true, dataState: 'published',
    publishedContent: content, ...overrides,
  }
}

const cleanPublicAttrs = {
  version: 'Revision 3', applicableModels: ['MODEL-A', 'MODEL-B'], resourceType: 'Datasheet',
  fileDescription: 'Published controlled document.', downloadUrl: '/media/public/controlled-file.pdf?revision=3',
  fileStatus: { scan: 'clean', access: 'public' },
}

describe('published download metadata', () => {
  it('accepts explicit metadata and does not put file URLs or status in list-card data', () => {
    const content = downloadContent(cleanPublicAttrs)
    expect(extractPublishedDownloadMetadata(content)).toMatchObject({
      version: 'Revision 3', applicableModels: ['MODEL-A', 'MODEL-B'], resourceType: 'Datasheet',
      downloadUrl: '/media/public/controlled-file.pdf?revision=3', downloadUrlState: 'safe',
      fileStatus: { scan: 'clean', access: 'public' },
    })
    const listMetadata = publishedDownloadListMetadata(content)
    expect(listMetadata).toEqual({
      version: 'Revision 3', applicableModels: ['MODEL-A', 'MODEL-B'], resourceType: 'Datasheet',
      fileDescription: 'Published controlled document.',
    })
    expect(listMetadata).not.toHaveProperty('downloadUrl')
    expect(listMetadata).not.toHaveProperty('fileStatus')
  })

  it('rejects the full applicable-model set when any value is malformed', () => {
    const metadata = extractPublishedDownloadMetadata(downloadContent({
      ...cleanPublicAttrs, applicableModels: ['MODEL-A', 'unsafe\u0000model'],
    }))
    expect(metadata?.applicableModels).toEqual([])
  })
})

describe('public download URL policy', () => {
  it.each([
    ['/media/public/file.pdf?revision=2', '/media/public/file.pdf?revision=2'],
    ['https://cdn.example.test/files/file.pdf', 'https://cdn.example.test/files/file.pdf'],
  ])('accepts an explicit public URL %s', (input, expected) => {
    expect(safePublicDownloadUrl(input)).toBe(expected)
  })

  it.each([
    'http://cdn.example.test/file.pdf', '//cdn.example.test/file.pdf', 'javascript:alert(1)',
    'data:application/pdf;base64,AAAA', 'mailto:files@example.test', '/admin/export',
    '/api/admin/v1/media/file', '/api%2Fadmin/v1/media/file', 'https://user:pass@cdn.example.test/file.pdf',
  ])('rejects a non-public or unsafe URL %s', (input) => {
    expect(safePublicDownloadUrl(input)).toBeUndefined()
  })
})

describe('download button eligibility', () => {
  it('allows only an indexable, non-placeholder, clean/public published record with a safe URL', () => {
    expect(publishedDownloadAvailability(page(downloadContent(cleanPublicAttrs)))).toEqual(expect.objectContaining({
      available: true, href: '/media/public/controlled-file.pdf?revision=3',
    }))
  })

  it.each([
    ['placeholder', downloadContent(cleanPublicAttrs, { placeholder: true }), {}],
    ['page noindex', downloadContent(cleanPublicAttrs), { indexable: false }],
    ['content noindex', downloadContent(cleanPublicAttrs, { indexable: false }), {}],
    ['not clean', downloadContent({ ...cleanPublicAttrs, fileStatus: { scan: 'pending', access: 'public' } }), {}],
    ['not public', downloadContent({ ...cleanPublicAttrs, fileStatus: { scan: 'clean', access: 'private' } }), {}],
    ['unsafe URL', downloadContent({ ...cleanPublicAttrs, downloadUrl: 'javascript:alert(1)' }), {}],
  ])('withholds the button for %s', (_label, content, overrides) => {
    const availability = publishedDownloadAvailability(page(content as ContentEntry, overrides as Partial<PublicPageModel>))
    expect(availability.available).toBe(false)
    expect(availability.href).toBeUndefined()
    expect(availability.reason).toBeTruthy()
  })
})
