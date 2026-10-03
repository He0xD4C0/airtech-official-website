import type { APIResponse } from '@playwright/test'

import { adminOrigin } from './environment'

// The API scopes the Secure attribute to the configured Admin origin: an http
// deployment must receive a browser-storable cookie, an https deployment must
// receive Secure. Asserting the wrong side silently breaks browser login in
// that environment, which is exactly what the http acceptance host hit.
const requiresSecureCookie = adminOrigin.startsWith('https://')

export interface BrowserCookie {
  name: string
  value: string
  domain: string
  path: string
  expires: number
  httpOnly: boolean
  secure: boolean
  sameSite: 'Strict' | 'Lax' | 'None'
}

function parseAttribute(attributes: string[], name: string): string | undefined {
  const prefix = `${name.toLowerCase()}=`
  return attributes
    .map((attribute) => attribute.trim())
    .find((attribute) => attribute.toLowerCase().startsWith(prefix))
    ?.slice(prefix.length)
}

export function secureHostOnlyCookies(response: APIResponse, hostname: string): BrowserCookie[] {
  const values = response.headersArray()
    .filter(({ name }) => name.toLowerCase() === 'set-cookie')
    .map(({ value }) => value)

  if (!values.length) throw new Error('The production API response did not include Set-Cookie.')

  return values.map((value) => {
    const parts = value.split(';')
    const pair = parts.shift()
    if (!pair) throw new Error('The production API returned an invalid Set-Cookie value.')
    const attributes = parts
    const separator = pair.indexOf('=')
    if (separator <= 0) throw new Error('The production API returned an invalid Set-Cookie value.')
    if (attributes.some((attribute) => attribute.trim().toLowerCase().startsWith('domain='))) {
      throw new Error('The production API session cookie must remain host-only.')
    }
    const secure = attributes.some((attribute) => attribute.trim().toLowerCase() === 'secure')
    if (secure !== requiresSecureCookie) {
      throw new Error(
        requiresSecureCookie
          ? 'The HTTPS production API session cookie must retain Secure.'
          : 'The HTTP E2E API session cookie must omit Secure so browsers store it.',
      )
    }
    const sameSite = parseAttribute(attributes, 'samesite')
    if (sameSite?.toLowerCase() !== 'strict') {
      throw new Error('The production API session cookie must retain SameSite=Strict in the E2E stack.')
    }
    const maxAge = Number.parseInt(parseAttribute(attributes, 'max-age') ?? '', 10)
    return {
      name: pair.slice(0, separator),
      value: pair.slice(separator + 1),
      domain: hostname,
      path: parseAttribute(attributes, 'path') ?? '/',
      expires: Number.isFinite(maxAge) ? Math.floor(Date.now() / 1000) + maxAge : -1,
      httpOnly: attributes.some((attribute) => attribute.trim().toLowerCase() === 'httponly'),
      secure,
      sameSite: 'Strict' as const,
    }
  })
}

export function cookieRequestHeader(cookies: BrowserCookie[]): string {
  return cookies.map(({ name, value }) => `${name}=${value}`).join('; ')
}

export function mergeCookies(current: BrowserCookie[], replacements: BrowserCookie[]): BrowserCookie[] {
  const replacementNames = new Set(replacements.map(({ name }) => name))
  return [...current.filter(({ name }) => !replacementNames.has(name)), ...replacements]
}
