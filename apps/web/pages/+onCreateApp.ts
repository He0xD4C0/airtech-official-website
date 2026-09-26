import { createPinia } from 'pinia'
import type { PageContext } from 'vike/types'
import { configurePublicRuntime, type PublicRuntimeConfig } from '@/shared/lib/runtimeConfig'
import { createPublicI18n } from '@/i18n'

export function onCreateApp(pageContext: PageContext) {
  if (pageContext.isRenderingHead) return
  const data = pageContext.data as { runtimeConfig?: PublicRuntimeConfig } | undefined
  if (data?.runtimeConfig) configurePublicRuntime(data.runtimeConfig)
  pageContext.app?.use(createPinia())
  pageContext.app?.use(createPublicI18n())
}
