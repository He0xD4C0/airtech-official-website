import { randomUUID } from 'node:crypto'
import { request } from '@playwright/test'

type ApiContext = Awaited<ReturnType<typeof request.newContext>>

function record(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null ? value as Record<string, unknown> : {}
}

async function expectRecord(
  response: Awaited<ReturnType<ApiContext['post']>>,
  action: string,
): Promise<Record<string, unknown>> {
  if (!response.ok()) throw new Error(`${action} (${response.status()}): ${await response.text()}`)
  return await response.json() as Record<string, unknown>
}

function matches(document: Record<string, unknown>, draft: Record<string, unknown>): boolean {
  return document.kind === draft.kind
    && (document.slug ?? null) === (draft.slug ?? null)
    && document.locale === 'en'
}

export async function upsertAndPublish(
  api: ApiContext,
  csrf: string,
  draft: Record<string, unknown>,
): Promise<void> {
  const publishedResponse = await api.get('/api/admin/v1/published-content?limit=100')
  if (!publishedResponse.ok()) {
    throw new Error(`Unable to inspect published fixtures (${publishedResponse.status()}): ${await publishedResponse.text()}`)
  }
  const published = await publishedResponse.json() as { items: Array<Record<string, unknown>> }
  if (published.items.some((item) => matches(record(item.document), draft))) return

  const draftsResponse = await api.get('/api/admin/v1/content-drafts?limit=100')
  if (!draftsResponse.ok()) {
    throw new Error(`Unable to inspect private draft fixtures (${draftsResponse.status()}): ${await draftsResponse.text()}`)
  }
  const drafts = await draftsResponse.json() as { items: Array<Record<string, unknown>> }
  let privateDraft = drafts.items.find((item) => matches(record(item.document), draft))
  if (!privateDraft) {
    const created = await api.post('/api/admin/v1/content-drafts', {
      headers: { 'X-CSRF-Token': csrf, 'Idempotency-Key': randomUUID() },
      data: draft,
    })
    privateDraft = await expectRecord(created, `Unable to create ${String(draft.kind)} private fixture`)
  }
  const submitted = await api.post(
    `/api/admin/v1/content-drafts/${String(privateDraft.draftId)}/submit`,
    {
      headers: {
        'X-CSRF-Token': csrf,
        'Idempotency-Key': randomUUID(),
        'If-Match': `"draft-${Number(privateDraft.draftVersion)}"`,
      },
    },
  )
  const result = await expectRecord(submitted, `Unable to publish ${String(draft.kind)} fixture`)
  if (result.status !== 'published') {
    throw new Error(`E2E fixture was not auto-published: ${JSON.stringify(result)}`)
  }
}
