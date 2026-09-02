import { describe, expect, it } from 'vitest'
import { safePublicDownloadUrl, sanitizeContentDocumentAttrs } from './contentDocumentAttrs'

describe('content document root attributes', () => {
  it('round-trips only the published Article allowlist', () => {
    const attrs = sanitizeContentDocumentAttrs('article', {
      author: '  Engineering   Editorial Team ',
      authorType: 'Organization',
      publishedAt: '2026-08-31T09:30:00+08:00',
      category: ' Engineering notes ',
      onclick: 'alert(1)',
      style: 'display:none',
      script: '<script>alert(1)</script>',
    })
    expect(attrs).toEqual({
      author: 'Engineering Editorial Team',
      authorType: 'Organization',
      publishedAt: '2026-08-31T09:30:00+08:00',
      category: 'Engineering notes',
    })
    expect(sanitizeContentDocumentAttrs('article', JSON.parse(JSON.stringify(attrs)))).toEqual(attrs)
  })

  it('round-trips only safe Download metadata and normalizes the model list', () => {
    const attrs = sanitizeContentDocumentAttrs('download', {
      version: ' Revision 3 ',
      applicableModels: ['MODEL-A', 'MODEL-A', 'MODEL-B'],
      resourceType: 'Datasheet',
      fileDescription: 'Published controlled document.',
      downloadUrl: '/media/public/document.pdf?revision=3',
      fileStatus: { scan: 'clean', access: 'public', executable: true },
      iframe: '<iframe src="https://invalid.example"></iframe>',
    })
    expect(attrs).toEqual({
      version: 'Revision 3',
      applicableModels: ['MODEL-A', 'MODEL-B'],
      resourceType: 'Datasheet',
      fileDescription: 'Published controlled document.',
      downloadUrl: '/media/public/document.pdf?revision=3',
      fileStatus: { scan: 'clean', access: 'public' },
    })
    expect(sanitizeContentDocumentAttrs('download', JSON.parse(JSON.stringify(attrs)))).toEqual(attrs)
  })

  it.each([
    'javascript:alert(1)',
    'http://downloads.example/file.pdf',
    'https://user:secret@downloads.example/file.pdf',
    '//downloads.example/file.pdf',
    '/admin/export.pdf',
    '/api/admin/v1/media/file.pdf',
    '/api/devtools/v1/file',
    '/api/internal/file.pdf',
    '/%61dmin/export.pdf',
  ])('rejects an unsafe download URL: %s', (url) => {
    expect(safePublicDownloadUrl(url)).toBeUndefined()
  })

  it('accepts only the two public URL forms', () => {
    expect(safePublicDownloadUrl('/media/public/file.pdf#v2')).toBe('/media/public/file.pdf#v2')
    expect(safePublicDownloadUrl('/api/public/v1/downloads/file.pdf')).toBe('/api/public/v1/downloads/file.pdf')
    expect(safePublicDownloadUrl('https://downloads.example/file.pdf')).toBe('https://downloads.example/file.pdf')
  })

  it('drops invalid dates, partial author identity, malformed model arrays, and all FAQ free-form attrs', () => {
    expect(sanitizeContentDocumentAttrs('article', { authorType: 'Person', publishedAt: '2026-02-30' })).toEqual({})
    expect(sanitizeContentDocumentAttrs('download', { applicableModels: ['MODEL-A', 'unsafe\u0000model'] })).toEqual({})
    expect(sanitizeContentDocumentAttrs('faq', { questions: [{ html: '<script>alert(1)</script>' }] })).toEqual({})
  })
})
