// Helpers for the acceptance stack's outbound capture services: Mailpit for
// SMTP and the local stub for the Aliyun SMS endpoint.
import type { APIRequestContext } from '@playwright/test'

const mailpitOrigin = process.env.E2E_MAILPIT_ORIGIN ?? 'http://127.0.0.1:18025'
const smsStubOrigin = process.env.E2E_SMS_STUB_ORIGIN ?? 'http://127.0.0.1:18099'

export const adminPhone = process.env.E2E_ADMIN_PHONE ?? '+8613800138000'

/// Points outbound email at the Mailpit SMTP listener and outbound SMS at the
/// stub endpoint. Both are configured through the real Admin API.
export async function configureDeliverySettings(
  api: APIRequestContext,
  csrf: string,
): Promise<void> {
  const mail = await api.put('/api/admin/v1/settings/mail', {
    headers: { 'X-CSRF-Token': csrf, 'If-Match': '"revision-0"' },
    data: {
      host: 'mailpit',
      port: 1025,
      protocol: 'plain',
      username: '',
      password: '',
      fromAddress: 'no-reply@airtek.invalid',
      fromName: 'AIRTEKPOWER E2E',
      reason: 'Route acceptance email through the isolated Mailpit capture service.',
    },
  })
  if (!mail.ok()) {
    throw new Error(`Unable to configure E2E mail capture (${mail.status()}): ${await mail.text()}`)
  }
  const sms = await api.put('/api/admin/v1/settings/sms', {
    headers: { 'X-CSRF-Token': csrf, 'If-Match': '"revision-0"' },
    data: {
      provider: 'aliyun',
      accessKeyId: 'e2e-access-key',
      accessKeySecret: 'e2e-access-secret',
      signName: 'AIRTEKPOWER E2E',
      templateCode: 'SMS_E2E',
      region: 'cn-hangzhou',
      reason: 'Route acceptance SMS through the isolated provider stub.',
    },
  })
  if (!sms.ok()) {
    throw new Error(`Unable to configure E2E SMS capture (${sms.status()}): ${await sms.text()}`)
  }
}

interface MailpitMessage {
  ID: string
  To?: Array<{ Address?: string }>
}

async function poll<T>(read: () => Promise<T | undefined>, description: string): Promise<T> {
  const deadline = Date.now() + 20_000
  while (Date.now() < deadline) {
    const value = await read()
    if (value !== undefined) return value
    await new Promise((resolve) => setTimeout(resolve, 500))
  }
  throw new Error(`Timed out waiting for ${description}.`)
}

function sixDigitCode(body: string): string | undefined {
  return /(?:^|\D)([0-9]{6})(?:\D|$)/u.exec(body)?.[1]
}

/// Reads the newest sign-in code that Mailpit captured for one recipient.
export async function waitForMailCode(
  request: APIRequestContext,
  to: string,
): Promise<string> {
  return await poll(async () => {
    const listed = await request.get(`${mailpitOrigin}/api/v1/messages?limit=100`)
    if (!listed.ok()) return undefined
    const payload = await listed.json() as { messages?: MailpitMessage[] }
    const message = (payload.messages ?? []).find((candidate) =>
      (candidate.To ?? []).some((recipient) =>
        (recipient.Address ?? '').toLowerCase() === to.toLowerCase()),
    )
    if (!message) return undefined
    const detail = await request.get(`${mailpitOrigin}/api/v1/message/${message.ID}`)
    if (!detail.ok()) return undefined
    const body = await detail.json() as { Text?: string; HTML?: string }
    return sixDigitCode(`${body.Text ?? ''}\n${body.HTML ?? ''}`)
  }, `the sign-in code addressed to ${to}`)
}

/// Reads the newest code the SMS stub received for one phone number.
export async function waitForSmsCode(
  request: APIRequestContext,
  phone: string,
): Promise<string> {
  return await poll(async () => {
    const response = await request.get(
      `${smsStubOrigin}/codes/latest?phone=${encodeURIComponent(phone)}`,
    )
    if (!response.ok()) return undefined
    const payload = await response.json() as { code?: string }
    return payload.code
  }, `the SMS code delivered to ${phone}`)
}
