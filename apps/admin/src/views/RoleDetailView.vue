<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowLeft, KeyRound, Save, ShieldCheck } from 'lucide-vue-next'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { adminApi, type AdminRoleRecord } from '@/services/adminApi'
import { apiErrorMessage, apiProblemStatus } from '@/services/cursorPagination'
import { useUiStore } from '@/stores/ui'

const route = useRoute()
const router = useRouter()
const ui = useUiStore()
const role = ref<AdminRoleRecord>()
const roleDefinitions = ref<AdminRoleRecord[]>([])
const displayName = ref('')
const selectedPermissions = ref<string[]>([])
const reason = ref('')
const saving = ref(false)
const state = ref<'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>('loading')
const availablePermissions = computed(() => [...new Set(roleDefinitions.value.flatMap((item) => item.permissions).concat(selectedPermissions.value))].sort())

async function load(): Promise<void> {
  state.value = 'loading'
  try {
    const [record, page] = await Promise.all([
      adminApi.getRole(String(route.params.id)),
      adminApi.listRoles({ limit: 100 }),
    ])
    role.value = record.role
    roleDefinitions.value = page.items
    displayName.value = record.role.displayName
    selectedPermissions.value = [...record.role.permissions]
    state.value = 'ready'
  } catch (error) {
    const status = apiProblemStatus(error)
    state.value = status === 403 ? 'forbidden' : status === 404 ? 'empty' : 'error'
  }
}

async function save(): Promise<void> {
  if (!role.value || saving.value) return
  if (reason.value.trim().length < 10) {
    ui.toast('需要变更原因', '审计原因至少需要 10 个字符。', 'warning')
    return
  }
  saving.value = true
  try {
    const result = await adminApi.updateRole(role.value.id, role.value.revision, {
      displayName: displayName.value.trim(),
      permissions: selectedPermissions.value,
      reason: reason.value.trim(),
    })
    role.value = result.role
    displayName.value = result.role.displayName
    selectedPermissions.value = [...result.role.permissions]
    reason.value = ''
    ui.toast('角色已更新', `Revision ${result.role.revision} 已写入审计。`)
  } catch (error) {
    ui.toast('角色更新失败', apiErrorMessage(error, '请重新加载最新 ETag 后重试。'), 'danger')
  } finally {
    saving.value = false
  }
}

onMounted(load)
</script>

<template><div class="page-stack"><PageHeader eyebrow="ROLE DETAIL" :title="role?.displayName ?? '角色详情'" description="权限矩阵来自 PostgreSQL；前端展示不构成授权边界。"><template #actions><button class="button button--secondary" type="button" @click="router.push('/roles')"><ArrowLeft :size="16" />返回</button><button class="button button--primary" type="button" :disabled="state !== 'ready' || saving" @click="save"><Save :size="16" />{{ saving ? '保存中…' : '保存角色' }}</button></template></PageHeader><DataStatePanel v-if="state !== 'ready'" :state="state" :title="state === 'empty' ? '角色不存在' : ''" @retry="load" /><template v-else-if="role"><div class="security-baseline"><ShieldCheck :size="18" /><div><strong>{{ role.key }}</strong><p>变更使用 revision ETag；每次 API 请求仍由服务端重新检查实际权限。</p></div><StatusBadge :label="role.systemRole ? 'System role' : 'Custom role'" :tone="role.systemRole ? 'warning' : 'neutral'" /></div><section class="panel settings-panel"><div class="form-grid"><label class="field"><span>角色名称</span><input v-model="displayName" /></label><label class="field"><span>Revision</span><input :value="role.revision" disabled /></label><label class="field"><span>变更原因</span><textarea v-model="reason" rows="3" placeholder="至少 10 个字符，将写入审计。"></textarea></label></div></section><section class="panel permission-matrix"><header><KeyRound :size="20" /><div><p class="eyebrow">PERMISSIONS</p><h2>{{ selectedPermissions.length }} 项授权</h2></div></header><fieldset class="role-options"><legend class="sr-only">权限矩阵</legend><label v-for="permission in availablePermissions" :key="permission"><input v-model="selectedPermissions" type="checkbox" :value="permission" /><span><code>{{ permission }}</code></span></label></fieldset></section></template></div></template>
