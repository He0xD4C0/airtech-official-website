import { describe, expect, it, vi } from 'vitest'
import { siteBootstrapResponse } from '@/lib/publicApiDecoders'
import { renderSiteIcon, renderWebManifest } from './siteMetadata'

function projection(kind: 'generalInformation' | 'navigation' | 'footer', typeFields: Record<string, unknown>) {
  return {
    schemaVersion: 2,
    id: crypto.randomUUID(),
    kind,
    templateKey: kind,
    slug: null,
    locale: 'en',
    title: kind,
    summary: null,
    body: null,
    composition: { blocks: [] },
    typeFields,
    seo: { title: null, description: null, indexable: false, socialImage: null },
    isPlaceholder: false,
    publishedRevision: 1,
    updatedAt: '2026-09-01T00:00:00Z',
    resolvedRelations: [],
    resolvedLinks: [],
    resolvedMedia: [] as Array<Record<string, unknown>>,
  }
}

function bootstrapWithIcon() {
  const iconId = crypto.randomUUID()
  const generalInformation = projection('generalInformation', {
    type: 'generalInformation',
    organizationName: 'Configured Brand',
    brandLine: null,
    siteIcon: { assetId: iconId },
    homePath: '/en',
    footerStatement: null,
    copyrightTemplate: 'Copyright {year}',
    contact: { email: null, phone: null, addressLines: [], locality: null, region: null, postalCode: null, countryCode: null },
    socialLinks: [],
    defaultSeo: { title: null, description: null, indexable: false, socialImage: null },
    productCategories: [],
    navigationCta: null,
  })
  generalInformation.resolvedMedia = [{
    assetId: iconId,
    publicUrl: 'https://media.example.test/icon/v7.png',
    previewUrl: null,
    downloadUrl: `/api/public/v1/media/${iconId}/download`,
    originalName: 'icon.png',
    mediaType: 'image/png',
    byteSize: 4096,
    originalWidth: 1024,
    originalHeight: 1024,
    previewWidth: null,
    previewHeight: null,
    previewByteSize: null,
  }]
  return {
    generalInformation,
    navigation: projection('navigation', { type: 'navigation', items: [] }),
    footer: projection('footer', { type: 'footer', columns: [], legalLinks: [] }),
    productFamilies: [],
    motorTechnologies: [],
    generatedAt: '2026-09-01T00:00:00Z',
  }
}

describe('public site metadata endpoints', () => {
  it('redirects to the exact published media version and describes its dimensions', async () => {
    const payload = bootstrapWithIcon()
    expect(() => siteBootstrapResponse(payload)).not.toThrow()
    const fetchImpl = vi.fn<typeof fetch>().mockImplementation(async () => new Response(JSON.stringify(payload), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    }))
    const icon = await renderSiteIcon({ fetchImpl })
    expect(icon.status).toBe(307)
    expect(icon.headers.get('location')).toBe('https://media.example.test/icon/v7.png')

    const manifest = await (await renderWebManifest({ fetchImpl })).json()
    expect(manifest.name).toBe('Configured Brand')
    expect(manifest.icons).toEqual([{
      src: '/site-icon', sizes: '1024x1024', type: 'image/png', purpose: 'any',
    }])
  })

  it('uses the neutral icon when the projection is unavailable', async () => {
    const fetchImpl = vi.fn<typeof fetch>().mockRejectedValue(new Error('offline'))
    const icon = await renderSiteIcon({ fetchImpl })
    expect(icon.headers.get('location')).toBe('/site-icon-placeholder.svg')

    const manifest = await (await renderWebManifest({ fetchImpl })).json()
    expect(manifest.icons[0]).toMatchObject({
      src: '/site-icon-placeholder.svg', sizes: 'any', type: 'image/svg+xml',
    })
  })
})
