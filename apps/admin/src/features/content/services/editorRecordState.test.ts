import { describe, expect, it } from 'vitest'
import { canPersistEditorRecord, editorFailureState } from '@/features/content/services/editorRecordState'

describe('editor record state', () => {
  it('keeps forbidden and missing records out of the editable state', () => {
    expect(editorFailureState(403)).toBe('forbidden')
    expect(editorFailureState(404)).toBe('empty')
    expect(editorFailureState(503)).toBe('error')
  })

  it('never converts a failed existing editor URL into a create request', () => {
    expect(canPersistEditorRecord('loading', false, undefined)).toBe(false)
    expect(canPersistEditorRecord('empty', false, undefined)).toBe(false)
    expect(canPersistEditorRecord('error', false, undefined)).toBe(false)
    expect(canPersistEditorRecord('forbidden', false, undefined)).toBe(false)
    expect(canPersistEditorRecord('ready', false, undefined)).toBe(false)
    expect(canPersistEditorRecord('ready', false, 'content-id')).toBe(true)
    expect(canPersistEditorRecord('ready', true, undefined)).toBe(true)
  })
})
