import { createPinia } from 'pinia'
import type { PageContext } from 'vike/types'
import { configurePublicRuntime, type PublicRuntimeConfig } from '@/shared/lib/runtimeConfig'

export function onCreateApp(pageContext: PageContext) {
  if (pageContext.isRenderingHead) return
  const data = pageContext.data as { runtimeConfig?: PublicRuntimeConfig } | undefined
  if (data?.runtimeConfig) configurePublicRuntime(data.runtimeConfig)
  pageContext.app?.use(createPinia())
}
