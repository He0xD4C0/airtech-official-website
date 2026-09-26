import { request as httpRequest } from 'node:http'
import { request as httpsRequest } from 'node:https'

const publicOrigin = process.env.PUBLIC_SITE_URL ?? 'http://127.0.0.1:3000'
const adminOrigin = process.env.ADMIN_SITE_URL ?? 'http://127.0.0.1:3100'
const apiOrigin = process.env.API_ORIGIN ?? 'http://127.0.0.1:8080'
const gatewayOrigin = process.env.GATEWAY_ORIGIN ?? 'http://127.0.0.1:8088'
const gatewayHosts = {
  public: process.env.PUBLIC_HOST ?? 'www.localhost',
  admin: process.env.ADMIN_HOST ?? 'admin.localhost',
  api: process.env.API_HOST ?? 'api.localhost',
}

const checks = [
  {
    name: 'public blocks admin route',
    url: `${publicOrigin}/admin`,
    expectStatus: 404,
  },
  {
    name: 'public root uses the canonical locale redirect',
    url: `${publicOrigin}/`,
    expectStatus: 308,
    expectHeader: ['location', '/en'],
  },
  {
    name: 'admin blocks public locale route',
    url: `${adminOrigin}/en`,
    expectStatus: 404,
  },
  {
    name: 'admin has no sitemap',
    url: `${adminOrigin}/sitemap.xml`,
    expectStatus: 404,
  },
  {
    name: 'admin has no sitemap shard',
    url: `${adminOrigin}/sitemap-products.xml`,
    expectStatus: 404,
  },
  {
    name: 'admin has no public manifest',
    url: `${adminOrigin}/site.webmanifest`,
    expectStatus: 404,
  },
  {
    name: 'admin has no webmaster verification file',
    url: `${adminOrigin}/google-airtek-verification.html`,
    expectStatus: 404,
  },
  {
    name: 'admin build manifest is not public',
    url: `${adminOrigin}/.vite/manifest.json`,
    expectStatus: 404,
  },
  {
    name: 'api has no sitemap',
    url: `${apiOrigin}/sitemap.xml`,
    expectStatus: 404,
  },
  {
    name: 'production API has no DevTools route',
    url: `${apiOrigin}/api/devtools/v1/sessions/token`,
    expectStatus: 404,
    method: 'POST',
  },
]

const failures = []

function requestGateway(path, host) {
  const url = new URL(path, gatewayOrigin)
  const request = url.protocol === 'https:' ? httpsRequest : httpRequest

  return new Promise((resolve, reject) => {
    const outgoing = request(url, { headers: { Host: host } }, (incoming) => {
      incoming.resume()
      incoming.once('end', () => {
        resolve({
          status: incoming.statusCode ?? 0,
          headers: {
            get(name) {
              const value = incoming.headers[name.toLowerCase()]
              return Array.isArray(value) ? value.join(', ') : (value ?? null)
            },
            has(name) {
              return incoming.headers[name.toLowerCase()] !== undefined
            },
          },
        })
      })
    })

    outgoing.once('error', reject)
    outgoing.end()
  })
}

for (const check of checks) {
  try {
    const response = await fetch(check.url, { method: check.method, redirect: 'manual' })
    if (response.status !== check.expectStatus) {
      failures.push(`${check.name}: expected ${check.expectStatus}, received ${response.status}`)
    }
    if (check.expectHeader) {
      const [name, value] = check.expectHeader
      if (response.headers.get(name) !== value) failures.push(`${check.name}: expected ${name}: ${value}`)
    }
  } catch (error) {
    failures.push(`${check.name}: ${error instanceof Error ? error.message : String(error)}`)
  }
}

for (const path of ['/robots.txt', '/login', '/assets/not-present.js']) {
  try {
    const response = await fetch(`${adminOrigin}${path}`, { redirect: 'manual' })
    for (const header of [
      'x-robots-tag',
      'x-content-type-options',
      'x-frame-options',
      'referrer-policy',
      'permissions-policy',
      'content-security-policy',
    ]) {
      if (!response.headers.has(header)) failures.push(`admin ${path} is missing ${header}`)
    }
  } catch (error) {
    failures.push(`admin ${path} headers: ${error instanceof Error ? error.message : String(error)}`)
  }
}

const gatewayChecks = [
  ['gateway public blocks admin', gatewayHosts.public, '/admin', 404],
  ['gateway admin blocks public locale', gatewayHosts.admin, '/en', 404],
  ['gateway admin blocks sitemap shard', gatewayHosts.admin, '/sitemap-products.xml', 404],
  ['gateway admin blocks manifest', gatewayHosts.admin, '/site.webmanifest', 404],
  ['gateway admin blocks webmaster verification', gatewayHosts.admin, '/google-airtek-verification.html', 404],
  ['gateway admin blocks Vite manifest', gatewayHosts.admin, '/.vite/manifest.json', 404],
  ['gateway API blocks sitemap', gatewayHosts.api, '/sitemap.xml', 404],
  ['gateway API blocks DevTools', gatewayHosts.api, '/api/devtools/v1/terminal', 404],
]

for (const [name, host, path, expectedStatus] of gatewayChecks) {
  try {
    const response = await requestGateway(path, host)
    if (response.status !== expectedStatus) failures.push(`${name}: expected ${expectedStatus}, received ${response.status}`)
  } catch (error) {
    failures.push(`${name}: ${error instanceof Error ? error.message : String(error)}`)
  }
}

for (const [name, host, path] of [
  ['gateway default health', 'unmatched.invalid', '/healthz'],
  ['gateway public rejection', gatewayHosts.public, '/admin'],
  ['gateway admin rejection', gatewayHosts.admin, '/en'],
  ['gateway API rejection', gatewayHosts.api, '/sitemap.xml'],
]) {
  try {
    const response = await requestGateway(path, host)
    if (name === 'gateway default health' && response.status !== 200) {
      failures.push(`${name}: expected 200, received ${response.status}`)
    }
    for (const header of [
      'x-content-type-options',
      'x-frame-options',
      'referrer-policy',
      'permissions-policy',
      'content-security-policy',
    ]) {
      if (!response.headers.has(header)) failures.push(`${name} is missing ${header}`)
    }
    if (host !== gatewayHosts.public && !/noindex/iu.test(response.headers.get('x-robots-tag') ?? '')) {
      failures.push(`${name} is missing X-Robots-Tag: noindex`)
    }
  } catch (error) {
    failures.push(`${name} headers: ${error instanceof Error ? error.message : String(error)}`)
  }
}

for (const [name, origin] of [['admin', adminOrigin], ['api', apiOrigin]]) {
  try {
    const response = await fetch(`${origin}/robots.txt`)
    const body = await response.text()
    if (response.status !== 200 || !/user-agent:\s*\*[\s\S]*disallow:\s*\//iu.test(body)) {
      failures.push(`${name} robots.txt does not disallow all crawlers`)
    }
    if (name === 'admin' && !/noindex/iu.test(response.headers.get('x-robots-tag') ?? '')) {
      failures.push('admin responses are missing X-Robots-Tag: noindex')
    }
  } catch (error) {
    failures.push(`${name} robots check: ${error instanceof Error ? error.message : String(error)}`)
  }
}

if (failures.length) {
  console.error(failures.map((failure) => `- ${failure}`).join('\n'))
  process.exit(1)
}

console.log('Runtime origin-isolation checks passed.')
