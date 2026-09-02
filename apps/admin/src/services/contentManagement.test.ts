import { describe, expect, it } from 'vitest'
import { isGenericContentKind } from './contentManagement'

describe('generic content management boundary', () => {
  it('routes News exclusively through the dedicated News workflow', () => {
    expect(isGenericContentKind('news')).toBe(false)
    expect(isGenericContentKind('article')).toBe(true)
    expect(isGenericContentKind('home')).toBe(true)
  })
})
