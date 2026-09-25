import { describe, expect, it, vi } from 'vitest'
import { renderSiteIcon, renderWebManifest } from './siteAssets'

function bootstrap(siteIcon: unknown, resolvedMedia: unknown[] = []) {
  return {
    generalInformation: {
      typeFields: { type: 'generalInformation', organizationName: 'Published organization', siteIcon },
      resolvedMedia,
    },
  }
}

describe('dynamic site assets', () => {
  it('uses a neutral SVG when no configured published icon is available', async () => {
    const fetchImpl = vi.fn(async () => Response.json(bootstrap(null))) as unknown as typeof fetch
    const icon = await renderSiteIcon({ fetchImpl })
    expect(icon.status).toBe(200)
    expect(icon.headers.get('Content-Type')).toContain('image/svg+xml')
    expect(await icon.text()).not.toMatch(/AIRTEKPOWER|logo/iu)

    const manifest = await renderWebManifest({ fetchImpl })
    expect(manifest.headers.get('Content-Type')).toContain('application/manifest+json')
    expect(await manifest.json()).toMatchObject({
      name: 'Published organization',
      icons: [{ src: '/site-icon', type: 'image/svg+xml', sizes: 'any' }],
    })
  })

  it('redirects to the exact resolved asset and publishes its real manifest metadata', async () => {
    const assetId = '98000000-0000-4000-8000-000000000001'
    const fetchImpl = vi.fn(async () => Response.json(bootstrap({ assetId }, [{
      assetId,
      publicUrl: 'https://media.example.test/immutable/site-icon.png',
      mediaType: 'image/png',
      originalWidth: 1024,
      originalHeight: 1024,
    }]))) as unknown as typeof fetch
    const icon = await renderSiteIcon({ fetchImpl })
    expect(icon.status).toBe(308)
    expect(icon.headers.get('Location')).toBe('https://media.example.test/immutable/site-icon.png')

    const manifest = await renderWebManifest({ fetchImpl })
    expect(await manifest.json()).toMatchObject({
      icons: [{ src: '/site-icon', type: 'image/png', sizes: '1024x1024' }],
    })
  })

  it('fails safely when the projection is unavailable or invalid', async () => {
    const unavailable = vi.fn(async () => new Response('unavailable', { status: 503 })) as unknown as typeof fetch
    expect((await renderSiteIcon({ fetchImpl: unavailable })).status).toBe(200)
    const manifest = await renderWebManifest({ fetchImpl: unavailable })
    expect(await manifest.json()).toMatchObject({ icons: [{ type: 'image/svg+xml' }] })
  })
})
