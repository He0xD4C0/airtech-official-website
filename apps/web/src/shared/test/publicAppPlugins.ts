import { createPinia } from 'pinia'
import { createPublicI18n, type PublicUiLocale } from '@/i18n'

export function createPublicTestPlugins(locale: PublicUiLocale = 'en') {
  return [createPinia(), createPublicI18n(locale)]
}
