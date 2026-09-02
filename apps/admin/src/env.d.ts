/// <reference types="vite/client" />

declare const __AIRTEK_DEVTOOLS__: boolean

declare module 'virtual:devtools-routes' {
  import type { Component } from 'vue'
  import type { RouteRecordRaw } from 'vue-router'

  export const devtoolsRoutes: RouteRecordRaw[]
  export const devtoolsNavigation: Array<{
    label: string
    to: string
    icon: Component
    permission: 'devtools.shell'
    devOnly: true
  }>
  export const devtoolsPermissions: Array<'devtools.shell'>
}

interface ImportMetaEnv {
  readonly VITE_ADMIN_API_BASE_URL?: string
  readonly VITE_ENABLE_DEVTOOLS?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
