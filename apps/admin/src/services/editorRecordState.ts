export type EditorRecordState = 'loading' | 'ready' | 'empty' | 'error' | 'forbidden'

export function editorFailureState(status: number | undefined): Exclude<EditorRecordState, 'loading' | 'ready'> {
  if (status === 403) return 'forbidden'
  if (status === 404) return 'empty'
  return 'error'
}

export function canPersistEditorRecord(
  state: EditorRecordState,
  isNew: boolean,
  entryId: string | undefined,
): boolean {
  return state === 'ready' && (isNew || Boolean(entryId))
}
