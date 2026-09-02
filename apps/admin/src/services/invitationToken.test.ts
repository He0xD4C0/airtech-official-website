import { describe, expect, it, vi } from 'vitest'
import { readAndClearInvitationToken } from './invitationToken'

describe('invitation token URL handling', () => {
  it('clears the URL immediately while retaining the token only in caller memory', async () => {
    const clearUrl = vi.fn(async () => undefined)
    const localWrite = vi.fn()
    const sessionWrite = vi.fn()
    vi.stubGlobal('localStorage', { setItem: localWrite })
    vi.stubGlobal('sessionStorage', { setItem: sessionWrite })
    const token = 'a'.repeat(43)

    await expect(readAndClearInvitationToken(token, clearUrl)).resolves.toBe(token)
    expect(clearUrl).toHaveBeenCalledOnce()
    expect(localWrite).not.toHaveBeenCalled()
    expect(sessionWrite).not.toHaveBeenCalled()
    vi.unstubAllGlobals()
  })

  it('does not rewrite a clean invitation URL', async () => {
    const clearUrl = vi.fn(async () => undefined)
    await expect(readAndClearInvitationToken(undefined, clearUrl)).resolves.toBe('')
    expect(clearUrl).not.toHaveBeenCalled()
  })
})
