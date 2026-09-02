let csrfTokenFromApi: string | undefined

function cookieValue(name: string): string | undefined {
  const prefix = `${encodeURIComponent(name)}=`
  return document.cookie
    .split(';')
    .map((part) => part.trim())
    .find((part) => part.startsWith(prefix))
    ?.slice(prefix.length)
}

export function captureAdminCsrfToken(response: Response): void {
  const rotatedCsrfToken = response.headers.get('X-CSRF-Token')
  if (rotatedCsrfToken) csrfTokenFromApi = rotatedCsrfToken
}

export function getAdminCsrfToken(): string | undefined {
  const token = csrfTokenFromApi ?? cookieValue('airtek_admin_csrf')
  return token ? decodeURIComponent(token) : undefined
}

export function clearAdminCsrfToken(): void {
  csrfTokenFromApi = undefined
}
