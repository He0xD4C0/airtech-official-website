import { computed, ref } from 'vue'
import type { ContentPublicationReadiness } from '@airtek/contracts'
import { contentApi } from '@/services/contentApi'
import { apiErrorMessage } from '@/services/cursorPagination'
import { useAuthStore } from '@/stores/auth'
import { useContentEditorStore } from '@/stores/contentEditor'
import { useUiStore } from '@/stores/ui'

const DEFAULT_REASON = 'Publish the current Admin CMS draft'

export function useContentPublication() {
  const auth = useAuthStore()
  const store = useContentEditorStore()
  const ui = useUiStore()
  const open = ref(false)
  const reason = ref(DEFAULT_REASON)
  const checking = ref(false)
  const publishing = ref(false)
  const readiness = ref<ContentPublicationReadiness | null>(null)

  const canPublish = computed(() => auth.hasPermission('content.publish'))
  const issues = computed(() => (
    readiness.value?.issues.map((issue) => `${issue.path}: ${issue.detail}`) ?? []
  ))

  async function prepare(): Promise<void> {
    if (!store.record || checking.value) return
    checking.value = true
    try {
      if (!(await store.flush())) {
        const detail = store.saveState === 'conflict'
          ? '发布前发现版本冲突，请先处理差异。'
          : store.saveError || '无法完成发布前保存。'
        ui.toast('无法准备发布', detail, 'warning')
        return
      }
      readiness.value = await contentApi.publicationReadiness(store.record.id)
      open.value = true
    } catch (error) {
      ui.toast('准备度读取失败', apiErrorMessage(error, '请检查权限与网络。'), 'danger')
    } finally {
      checking.value = false
    }
  }

  async function publish(): Promise<void> {
    publishing.value = true
    try {
      const result = await store.publish(reason.value.trim() || DEFAULT_REASON)
      if (result) {
        open.value = false
        ui.toast('内容已发布', `Published revision ${result.publishedRevision}.`)
      }
    } catch (error) {
      ui.toast('发布失败', apiErrorMessage(error, '请检查服务端阻塞项、权限与并发版本。'), 'danger')
    } finally {
      publishing.value = false
    }
  }

  function close(): void {
    if (!publishing.value) open.value = false
  }

  return { canPublish, checking, close, issues, open, prepare, publish, publishing, readiness, reason }
}
