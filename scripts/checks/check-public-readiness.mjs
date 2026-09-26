#!/usr/bin/env node

import { pathToFileURL } from 'node:url'

export const CORE_PUBLIC_PATHS = [
  '/en',
  '/en/products',
  '/en/products/selector',
  '/en/products/compare',
  '/en/search',
  '/en/company/about',
  '/en/company/contact',
  '/en/request-a-quote',
  '/en/request-a-quote/product',
  '/en/request-a-quote/selection',
  '/en/request-a-quote/project',
  '/en/request-a-quote/replacement',
  '/en/privacy',
  '/en/terms',
  '/en/cookie-settings',
]

const SITEMAPS = [
  '/sitemap-pages.xml',
  '/sitemap-products.xml',
  '/sitemap-solutions.xml',
  '/sitemap-resources.xml',
]

function normalizedOrigin(value, label) {
  let parsed
  try {
    parsed = new URL(value)
  } catch {
    throw new Error(`${label} must be an absolute HTTP(S) origin.`)
  }
  if (!['http:', 'https:'].includes(parsed.protocol)
    || parsed.username
    || parsed.password
    || parsed.pathname !== '/'
    || parsed.search
    || parsed.hash) {
    throw new Error(`${label} must be an absolute HTTP(S) origin without credentials or a path.`)
  }
  return parsed.origin
}

function parseArguments(argv, environment) {
  const values = new Map()
  let allowPlaceholders = false
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index]
    if (argument === '--allow-placeholders') {
      allowPlaceholders = true
      continue
    }
    if (!argument.startsWith('--') || !argv[index + 1]) {
      throw new Error(`Unsupported readiness argument: ${argument}`)
    }
    values.set(argument.slice(2), argv[index + 1])
    index += 1
  }
  const expectedPublicOrigin = normalizedOrigin(
    values.get('expected-public-origin') || environment.AIRTEK_PUBLIC_ORIGIN || environment.PUBLIC_ORIGIN,
    'expected public origin',
  )
  const expectedApiOrigin = normalizedOrigin(
    values.get('expected-api-origin') || environment.AIRTEK_API_ORIGIN || environment.PUBLIC_API_BROWSER_ORIGIN,
    'expected API origin',
  )
  return {
    publicBase: normalizedOrigin(values.get('public-base') || expectedPublicOrigin, 'public request base'),
    apiBase: normalizedOrigin(values.get('api-base') || expectedApiOrigin, 'API request base'),
    expectedPublicOrigin,
    expectedApiOrigin,
    allowPlaceholders,
  }
}

function xmlLocations(xml) {
  return [...xml.matchAll(/<loc>([^<]+)<\/loc>/gu)].map((match) => match[1])
}

async function responseBody(response) {
  try {
    return await response.text()
  } catch {
    return ''
  }
}

export async function checkPublicReadiness(options, fetchImpl = globalThis.fetch) {
  const errors = []
  const warnings = []
  const checked = []
  const request = async (base, path, init = {}) => {
    try {
      return await fetchImpl(`${base}${path}`, {
        redirect: 'manual',
        ...init,
        signal: AbortSignal.timeout(10_000),
      })
    } catch (error) {
      errors.push(`${path}: request failed: ${error instanceof Error ? error.message : String(error)}`)
      return null
    }
  }
  const expectStatus = async (base, path, expected, init) => {
    const response = await request(base, path, init)
    if (!response) return null
    if (!expected.includes(response.status)) {
      const detail = (await responseBody(response)).replace(/\s+/gu, ' ').trim().slice(0, 180)
      errors.push(`${path}: expected ${expected.join('/')} but received ${response.status}${detail ? ` (${detail})` : ''}.`)
      return null
    }
    checked.push(path)
    return response
  }

  const root = await expectStatus(options.publicBase, '/', [308])
  if (root && root.headers.get('location') !== '/en') errors.push('/: expected redirect location /en.')

  const runtimeConfigResponse = await expectStatus(options.publicBase, '/runtime-config.json', [200])
  if (runtimeConfigResponse) {
    if (!runtimeConfigResponse.headers.get('cache-control')?.includes('no-store')) {
      errors.push('/runtime-config.json: response must not be cached.')
    }
    try {
      const runtimeConfig = await runtimeConfigResponse.json()
      if (runtimeConfig.apiBaseUrl !== `${options.expectedApiOrigin}/api/public/v1`) {
        errors.push('/runtime-config.json: API base does not match the deployment origin.')
      }
      if (runtimeConfig.publicOrigin !== options.expectedPublicOrigin) {
        errors.push('/runtime-config.json: canonical origin does not match the deployment origin.')
      }
    } catch {
      errors.push('/runtime-config.json: response is not valid JSON.')
    }
  }

  const bootstrapResponse = await expectStatus(
    options.apiBase,
    '/api/public/v1/site-bootstrap?locale=en',
    [200],
    { headers: { Accept: 'application/json' } },
  )
  let bootstrap
  if (bootstrapResponse) {
    try {
      bootstrap = await bootstrapResponse.json()
    } catch {
      errors.push('site bootstrap: response is not valid JSON.')
    }
  }
  for (const key of ['generalInformation', 'navigation', 'footer']) {
    if (!bootstrap?.[key]) errors.push(`site bootstrap: ${key} is missing.`)
    if (!options.allowPlaceholders && bootstrap?.[key]?.isPlaceholder === true) {
      errors.push(`site bootstrap: ${key} is still a development placeholder.`)
    }
  }

  let homeHtml = ''
  for (const path of CORE_PUBLIC_PATHS) {
    const response = await expectStatus(options.publicBase, path, [200])
    if (response) {
      const html = await responseBody(response)
      if (path === '/en') homeHtml = html
      const canonical = `${options.expectedPublicOrigin}${path}`
      if (!html.includes(`rel="canonical"`) || !html.includes(`href="${canonical}"`)) {
        errors.push(`${path}: canonical URL does not use ${canonical}.`)
      }
      if (options.allowPlaceholders && !/<meta[^>]+name="robots"[^>]+noindex/iu.test(html)) {
        errors.push(`${path}: a permitted development placeholder is missing noindex.`)
      }
      const csp = response.headers.get('content-security-policy') || ''
      if (!csp.includes(options.expectedApiOrigin)) {
        errors.push(`${path}: CSP does not contain the expected browser API origin.`)
      }
    }

    const routeResponse = await expectStatus(
      options.apiBase,
      `/api/public/v1/routes/resolve?path=${encodeURIComponent(path)}&locale=en`,
      [200],
      { headers: { Accept: 'application/json' } },
    )
    if (!routeResponse) continue
    try {
      const route = await routeResponse.json()
      const placeholder = route.dataClass === 'developmentFixture' || route.page?.isPlaceholder === true
      if (!options.allowPlaceholders && placeholder) errors.push(`${path}: published route is a development placeholder.`)
      if (placeholder && route.indexable !== false) errors.push(`${path}: placeholder route is indexable.`)
    } catch {
      errors.push(`${path}: route projection is not valid JSON.`)
    }
  }

  const cors = await expectStatus(options.apiBase, '/api/public/v1/contact', [200, 204], {
    method: 'OPTIONS',
    headers: {
      Origin: options.expectedPublicOrigin,
      'Access-Control-Request-Method': 'POST',
      'Access-Control-Request-Headers': 'content-type,idempotency-key',
    },
  })
  if (cors?.headers.get('access-control-allow-origin') !== options.expectedPublicOrigin) {
    errors.push('CORS preflight does not allow the configured public origin.')
  }

  const robots = await expectStatus(options.publicBase, '/robots.txt', [200])
  if (robots) {
    const body = await responseBody(robots)
    if (!body.includes(`Sitemap: ${options.expectedPublicOrigin}/sitemap.xml`)) {
      errors.push('/robots.txt: production sitemap URL is missing or uses the wrong origin.')
    }
    if (/^Disallow: \/$/mu.test(body)) errors.push('/robots.txt: crawling is globally disabled.')
  }

  const index = await expectStatus(options.publicBase, '/sitemap.xml', [200])
  if (index) {
    const body = await responseBody(index)
    if (!body.includes('<sitemapindex')) errors.push('/sitemap.xml: response is not a sitemap index.')
    const locations = xmlLocations(body)
    for (const path of SITEMAPS) {
      if (!locations.includes(`${options.expectedPublicOrigin}${path}`)) {
        errors.push(`/sitemap.xml: missing ${options.expectedPublicOrigin}${path}.`)
      }
      const child = await expectStatus(options.publicBase, path, [200])
      if (!child) continue
      const childBody = await responseBody(child)
      if (!childBody.includes('<urlset')) errors.push(`${path}: response is not a URL sitemap.`)
      for (const location of xmlLocations(childBody)) {
        if (!location.startsWith(`${options.expectedPublicOrigin}/`)) {
          errors.push(`${path}: URL ${location} uses the wrong public origin.`)
        }
      }
    }
  }

  const icon = await expectStatus(options.publicBase, '/site-icon', [200, 307, 308])
  if (icon) {
    if (icon.status === 200) {
      if (!icon.headers.get('content-type')?.startsWith('image/')) {
        errors.push('/site-icon: direct response is not an image.')
      }
      else warnings.push('No custom site icon is published; the neutral fallback remains active.')
    }
    else {
      const location = icon.headers.get('location')
      if (!location) {
        errors.push('/site-icon: redirect target is missing.')
      }
      else {
        if (location === '/site-icon-placeholder.svg') warnings.push('No custom site icon is published; the neutral fallback remains active.')
        const target = location.startsWith('/') ? `${options.publicBase}${location}` : location
        const resolved = await request('', target)
        if (!resolved?.ok || !resolved.headers.get('content-type')?.startsWith('image/')) {
          errors.push('/site-icon: resolved icon is not a readable image.')
        }
      }
    }
  }

  const manifestResponse = await expectStatus(options.publicBase, '/site.webmanifest', [200])
  if (manifestResponse) {
    try {
      const manifest = await manifestResponse.json()
      if (!Array.isArray(manifest.icons) || manifest.icons.length === 0) {
        errors.push('/site.webmanifest: no icons are declared.')
      }
    } catch {
      errors.push('/site.webmanifest: response is not valid JSON.')
    }
  }

  return { ok: errors.length === 0, errors, warnings, checked }
}

async function main() {
  const options = parseArguments(process.argv.slice(2), process.env)
  const result = await checkPublicReadiness(options)
  for (const warning of result.warnings) console.warn(`WARNING: ${warning}`)
  if (!result.ok) {
    for (const error of result.errors) console.error(`ERROR: ${error}`)
    process.exitCode = 1
    return
  }
  console.log(`Public readiness passed (${result.checked.length} endpoint checks).`)
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error))
    process.exitCode = 1
  })
}
