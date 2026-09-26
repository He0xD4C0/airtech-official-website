import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from '@/app/App.vue'
import router from '@/app/router'
import '@airtek/ui/base.css'
import '@/app/styles/main.css'
import '@/app/styles/responsive.css'

const app = createApp(App)

app.use(createPinia())
app.use(router)
app.mount('#app')
