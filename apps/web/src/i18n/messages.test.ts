import { describe, expect, it } from 'vitest'
import { createPublicI18n, persistPublicUiLocale, PUBLIC_UI_LOCALES, storedPublicUiLocale } from '@/i18n'
import { en } from '@/i18n/messages/en'
import { zhCN } from '@/i18n/messages/zh-CN'

interface MessageTree {
  [key: string]: string | MessageTree
}

function flatten(tree: MessageTree, prefix = ''): Map<string, string> {
  const entries = new Map<string, string>()
  for (const [key, value] of Object.entries(tree)) {
    const path = prefix ? `${prefix}.${key}` : key
    if (typeof value === 'string') entries.set(path, value)
    else for (const [nestedKey, nestedValue] of flatten(value, path)) entries.set(nestedKey, nestedValue)
  }
  return entries
}

function interpolationNames(message: string): string[] {
  return [...new Set([...message.matchAll(/\{([^{}]+)\}/gu)].map((match) => match[1]!))].sort()
}

describe('public UI dictionaries', () => {
  it('has identical non-empty keys and interpolation variables', () => {
    const english = flatten(en)
    const chinese = flatten(zhCN)
    expect([...chinese.keys()].sort()).toEqual([...english.keys()].sort())
    for (const [key, message] of english) {
      const translation = chinese.get(key)
      expect(message.trim(), key).not.toBe('')
      expect(translation?.trim(), key).not.toBe('')
      expect(interpolationNames(translation ?? ''), key).toEqual(interpolationNames(message))
    }
  })

  it('resolves every runtime key in both locales without fallback', () => {
    const keys = [...flatten(en).keys()]
    for (const locale of PUBLIC_UI_LOCALES) {
      const i18n = createPublicI18n(locale)
      for (const key of keys) expect(i18n.global.te(key, locale), `${locale}:${key}`).toBe(true)
    }
  })

  it('defines every statically referenced translation key', () => {
    const sources = import.meta.glob('../**/*.{ts,vue}', {
      eager: true,
      query: '?raw',
      import: 'default',
    }) as Record<string, string>
    const defined = flatten(en)
    const missing = Object.entries(sources).flatMap(([file, source]) => (
      [...source.matchAll(/\bt\(\s*['"]([^'"]+)['"]/gu)]
        .map((match) => `${file}:${match[1]}`)
        .filter((reference) => !defined.has(reference.slice(reference.indexOf(':') + 1)))
    ))
    expect(missing).toEqual([])
  })

  it('accepts only the two versioned browser preference values', () => {
    expect(storedPublicUiLocale({ getItem: () => 'zh-CN' })).toBe('zh-CN')
    expect(storedPublicUiLocale({ getItem: () => 'en' })).toBe('en')
    expect(storedPublicUiLocale({ getItem: () => 'zh-cn' })).toBe('en')
    expect(storedPublicUiLocale({ getItem: () => 'javascript:alert(1)' })).toBe('en')
    expect(storedPublicUiLocale({ getItem: () => null })).toBe('en')
    expect(storedPublicUiLocale({ getItem: () => { throw new Error('blocked') } })).toBe('en')
    expect(() => persistPublicUiLocale({ setItem: () => { throw new Error('blocked') } }, 'zh-CN')).not.toThrow()
  })
})
