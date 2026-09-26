import { request as httpRequest } from 'node:http'

const gatewayOrigin = required('E2E_GATEWAY_CONTROL_ORIGIN')
const publicOrigin = required('E2E_PUBLIC_ORIGIN')
const apiControlOrigin = required('E2E_API_CONTROL_ORIGIN')
const publicHost = process.env.E2E_PUBLIC_GATEWAY_HOST || new URL(publicOrigin).host

function required(name) {
  const value = process.env[name]?.trim()
  if (!value) throw new Error(`${name} is required.`)
  return value.replace(/\/$/u, '')
}

async function gateway(path) {
  return await new Promise((resolve, reject) => {
    const request = httpRequest(`${gatewayOrigin}${path}`, {
      headers: { Host: publicHost },
    }, (response) => {
      const chunks = []
      response.on('data', (chunk) => chunks.push(chunk))
      response.on('end', () => resolve({
        status: response.statusCode ?? 0,
        text: async () => Buffer.concat(chunks).toString('utf8'),
      }))
    })
    request.setTimeout(10_000, () => request.destroy(new Error(`Timed out requesting ${path}.`)))
    request.on('error', reject)
    request.end()
  })
}

function assert(condition, detail) {
  if (!condition) throw new Error(detail)
}

const page = await gateway('/en')
assert(page.status === 503, `Empty public /en returned ${page.status}, expected 503.`)
const pageBody = await page.text()
assert(/temporarily unavailable/iu.test(pageBody), 'Empty public /en did not render the unavailable state.')

const bootstrap = await fetch(`${apiControlOrigin}/api/public/v1/site-bootstrap?locale=en`, {
  headers: { Origin: publicOrigin },
  redirect: 'manual',
  signal: AbortSignal.timeout(10_000),
})
assert(bootstrap.status === 503, `Empty site-bootstrap returned ${bootstrap.status}, expected 503.`)

const robots = await gateway('/robots.txt')
assert(robots.status === 200, `Empty robots.txt returned ${robots.status}.`)
const robotsBody = await robots.text()
assert(robotsBody.split(/\r?\n/u).some((line) => line.trim() === 'Disallow: /'), 'Empty robots.txt must disallow the full site.')
assert(!robotsBody.includes('Sitemap:'), 'Empty robots.txt must not advertise a sitemap.')

for (const path of [
  '/sitemap.xml',
  '/sitemap-pages.xml',
  '/sitemap-products.xml',
  '/sitemap-solutions.xml',
  '/sitemap-resources.xml',
]) {
  const response = await gateway(path)
  assert(response.status === 503, `Empty ${path} returned ${response.status}, expected 503.`)
}

for (const path of ['/site-icon', '/favicon.ico', '/site.webmanifest']) {
  const response = await gateway(path)
  assert(response.status === 200, `Fallback ${path} returned ${response.status}.`)
}

console.log('Empty production projection fails closed with usable neutral site assets.')
