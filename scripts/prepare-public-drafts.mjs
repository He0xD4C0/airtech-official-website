import { readFile } from 'node:fs/promises'
import { draftFromTemplate } from '../apps/admin/src/features/content/services/contentDraftDefaults.ts'

// Supply a fresh inspect-public-site report and an authenticated Admin session.
// Only missing entities are created. No save, review, or publish calls are made.
const report = JSON.parse(await readFile(process.argv[2], 'utf8'))
if (report.locale !== 'en' || report.readOnly !== true || !Array.isArray(report.entries)) throw new Error('Expected inspect-public-site inventory.')
const base = new URL(process.env.AIRTEK_DRAFT_API_ORIGIN)
if (base.protocol !== 'https:' && !['api.airtek.localhost', '127.0.0.1', 'localhost'].includes(base.hostname)) throw new Error('Admin API requires HTTPS outside loopback.')
for (const key of ['AIRTEK_DRAFT_SESSION_COOKIE', 'AIRTEK_DRAFT_CSRF_TOKEN', 'AIRTEK_DRAFT_ADMIN_ORIGIN']) {
  if (!process.env[key]) throw new Error(`${key} is required`)
}
async function api(path, body) {
  const response = await fetch(new URL(path, base), {
    method: body ? 'POST' : 'GET',
    headers: {
      Cookie: process.env.AIRTEK_DRAFT_SESSION_COOKIE, Origin: process.env.AIRTEK_DRAFT_ADMIN_ORIGIN,
      'X-CSRF-Token': process.env.AIRTEK_DRAFT_CSRF_TOKEN, 'Content-Type': 'application/json',
    },
    ...(body ? { body: JSON.stringify(body) } : {}),
    redirect: 'error',
  })
  if (!response.ok) throw new Error(`CMS request ${path} failed: ${response.status}`)
  return await response.json()
}
const templates = (await api('/api/admin/v1/content-drafts/templates')).items
for (const entry of report.entries) {
  if (!entry.missing || entry.records.length) continue
  const template = templates.find((item) => item.key === entry.templateKey)
  if (!template) throw new Error('Unknown CMS template.')
  const draft = draftFromTemplate({ template, title: entry.title, slug: entry.slug, isPlaceholder: true })
  const created = await api('/api/admin/v1/content-drafts', draft)
  console.log(JSON.stringify({ templateKey: entry.templateKey, draftId: created.draftId, status: 'draftOnly' }))
}
