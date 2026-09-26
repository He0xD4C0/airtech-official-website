export async function readAndClearInvitationToken(
  queryValue: unknown,
  clearUrl: () => Promise<unknown>,
): Promise<string> {
  const token = typeof queryValue === 'string' ? queryValue : ''
  if (token) await clearUrl()
  return token
}
