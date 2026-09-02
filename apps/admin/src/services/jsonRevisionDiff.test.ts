import { describe, expect, it } from 'vitest'
import { diffJsonRevisions } from './jsonRevisionDiff'

describe('diffJsonRevisions', () => {
  it('reports stable JSON paths for changed, added, and removed values', () => {
    expect(diffJsonRevisions(
      { title: 'Before', seo: { indexable: false, description: 'Old' }, removed: true },
      { title: 'After', seo: { indexable: false, description: 'New' }, added: true },
    )).toEqual([
      { path: '$.added', before: undefined, after: true },
      { path: '$.removed', before: true, after: undefined },
      { path: '$.seo.description', before: 'Old', after: 'New' },
      { path: '$.title', before: 'Before', after: 'After' },
    ])
  })

  it('treats arrays as one ordered JSON value', () => {
    expect(diffJsonRevisions({ links: ['a', 'b'] }, { links: ['b', 'a'] })).toEqual([{
      path: '$.links',
      before: ['a', 'b'],
      after: ['b', 'a'],
    }])
  })
})
