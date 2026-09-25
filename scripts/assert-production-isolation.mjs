import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join, relative } from 'node:path'

const root = dirname(dirname(fileURLToPath(import.meta.url)))
const adminDist = join(root, 'apps/admin/dist')
const publicDist = join(root, 'apps/web/dist')
const failures = []
const adminMockRecordMarkers = [
  'demo-not-for-publish',
  'demo-product-',
  'demo-rfq-',
  'audit-demo-',
  'op-demo-',
  'demo@localhost.invalid',
  'demo organization',
]
const adminDevtoolsBundleMarkers = [
  '/api/devtools',
  'devtools.shell',
  '/developer-tools',
  'terminal-shell',
  'airtekctl',
  'xterm',
]

function walk(directory) {
  if (!existsSync(directory)) return []
  return readdirSync(directory).flatMap((entry) => {
    const path = join(directory, entry)
    return statSync(path).isDirectory() ? walk(path) : [path]
  })
}

if (!existsSync(adminDist)) failures.push('Admin production output is missing.')
if (!existsSync(publicDist)) failures.push('Public production output is missing.')

for (const path of walk(adminDist)) {
  const name = relative(adminDist, path).toLowerCase()
  if (name.includes('sitemap')) failures.push(`Admin bundle exposes a sitemap file: ${name}`)
  if (/^(?:site\.webmanifest|manifest(?:\.webmanifest|\.json)|browserconfig\.xml)$/u.test(name)) {
    failures.push(`Admin bundle exposes a public manifest: ${name}`)
  }
  if (/^(?:google[^/]+\.html|bingsiteauth\.xml|baidu_verify[^/]*\.html|yandex_[^/]+\.html)$/u.test(name)) {
    failures.push(`Admin bundle exposes a webmaster verification file: ${name}`)
  }
  if (/\.(?:js|mjs|cjs|css|html|json|map|txt)$/u.test(path)) {
    const body = readFileSync(path, 'utf8').toLowerCase().replaceAll('\\/', '/')
    if (adminDevtoolsBundleMarkers.some((marker) => body.includes(marker))) {
      failures.push(`Admin production bundle contains a DevTools marker: ${name}`)
    }
    if (adminMockRecordMarkers.some((marker) => body.includes(marker))) {
      failures.push(`Admin production bundle contains mock record data: ${name}`)
    }
  }
}

const adminIndex = join(adminDist, 'index.html')
if (!existsSync(adminIndex)) {
  failures.push('Admin index.html is missing.')
} else if (!/<meta\s+name=["']robots["']\s+content=["'][^"']*noindex[^"']*nofollow/iu.test(readFileSync(adminIndex, 'utf8'))) {
  failures.push('Admin index.html does not contain noindex,nofollow metadata.')
}

const adminRobots = join(adminDist, 'robots.txt')
if (!existsSync(adminRobots)) {
  failures.push('Admin robots.txt is missing.')
} else if (!/user-agent:\s*\*[\s\S]*disallow:\s*\//iu.test(readFileSync(adminRobots, 'utf8'))) {
  failures.push('Admin robots.txt does not disallow all crawlers.')
}

for (const staticDiscoveryAsset of [
  'robots.txt',
  'sitemap.xml',
  'sitemap-pages.xml',
  'sitemap-products.xml',
  'sitemap-solutions.xml',
  'sitemap-resources.xml',
  'site.webmanifest',
  'site-icon',
  'favicon.ico',
]) {
  const clientPath = join(publicDist, 'client', staticDiscoveryAsset)
  const fallbackPath = join(publicDist, staticDiscoveryAsset)
  if (existsSync(clientPath) || existsSync(fallbackPath)) {
    failures.push(`Public production output contains static ${staticDiscoveryAsset}; it would bypass the dynamic publication handler.`)
  }
}

const publicServerText = walk(join(publicDist, 'server'))
  .filter((path) => /\.(?:js|mjs|cjs)$/u.test(path))
  .map((path) => readFileSync(path, 'utf8'))
  .join('\n')
for (const marker of ['/robots.txt', '/sitemap.xml', '/discovery', '/site-icon', '/site.webmanifest']) {
  if (!publicServerText.includes(marker)) {
    failures.push(`Public SSR bundle does not contain the dynamic ${marker} handler.`)
  }
}

if (failures.length) {
  console.error(failures.map((failure) => `- ${failure}`).join('\n'))
  process.exit(1)
}

console.log('Production isolation checks passed.')
