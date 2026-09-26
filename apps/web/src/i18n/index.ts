import { createI18n } from 'vue-i18n'
import { en } from '@/i18n/messages/en'
import { zhCN } from '@/i18n/messages/zh-CN'

export const PUBLIC_UI_LOCALES = ['en', 'zh-CN'] as const
export type PublicUiLocale = typeof PUBLIC_UI_LOCALES[number]
export const PUBLIC_UI_LOCALE_STORAGE_KEY = 'airtek.public.ui-locale.v1'

export function isPublicUiLocale(value: unknown): value is PublicUiLocale {
  return typeof value === 'string' && PUBLIC_UI_LOCALES.includes(value as PublicUiLocale)
}

export function storedPublicUiLocale(storage: Pick<Storage, 'getItem'>): PublicUiLocale {
  try {
    const value = storage.getItem(PUBLIC_UI_LOCALE_STORAGE_KEY)
    return isPublicUiLocale(value) ? value : 'en'
  } catch {
    return 'en'
  }
}

export function persistPublicUiLocale(storage: Pick<Storage, 'setItem'>, locale: PublicUiLocale): void {
  try {
    storage.setItem(PUBLIC_UI_LOCALE_STORAGE_KEY, locale)
  } catch {
    // The interface still switches when storage is unavailable or blocked.
  }
}

export function createPublicI18n(locale: PublicUiLocale = 'en') {
  return createI18n({
    legacy: false,
    locale,
    fallbackLocale: 'en',
    messages: { en, 'zh-CN': zhCN },
    missingWarn: import.meta.env.DEV,
    fallbackWarn: import.meta.env.DEV,
  })
}

export type PublicI18n = ReturnType<typeof createPublicI18n>
