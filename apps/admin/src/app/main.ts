import { createApp } from 'vue'
import { createPinia } from 'pinia'
import { initializeAdminRuntime } from '@/app/runtimeConfig'
import '@airtek/ui/base.css'
import '@/app/styles/main.css'
import '@/app/styles/responsive.css'

await initializeAdminRuntime()
const [{ default: App }, { default: router }] = await Promise.all([
  import('@/app/App.vue'),
  import('@/app/router'),
])
const app = createApp(App)

app.use(createPinia())
app.use(router)
app.mount('#app')
