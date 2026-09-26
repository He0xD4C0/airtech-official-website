import { ref } from 'vue'
import { defineStore } from 'pinia'

export interface ToastMessage {
  id: number
  title: string
  detail?: string
  tone: 'success' | 'warning' | 'danger' | 'info'
}

export const useUiStore = defineStore('ui', () => {
  const sidebarOpen = ref(false)
  const commandOpen = ref(false)
  const toasts = ref<ToastMessage[]>([])
  let nextToastId = 1

  function toggleSidebar(): void {
    sidebarOpen.value = !sidebarOpen.value
  }

  function closeSidebar(): void {
    sidebarOpen.value = false
  }

  function toast(title: string, detail?: string, tone: ToastMessage['tone'] = 'success'): void {
    const id = nextToastId++
    toasts.value.push({ id, title, detail, tone })
    window.setTimeout(() => dismissToast(id), 4200)
  }

  function dismissToast(id: number): void {
    toasts.value = toasts.value.filter((item) => item.id !== id)
  }

  return { sidebarOpen, commandOpen, toasts, toggleSidebar, closeSidebar, toast, dismissToast }
})
