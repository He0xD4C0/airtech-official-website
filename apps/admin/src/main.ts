import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import router from './router'
import '@airtek/ui/base.css'
import './styles/main.css'
import './styles/responsive.css'

const app = createApp(App)

app.use(createPinia())
app.use(router)
app.mount('#app')
