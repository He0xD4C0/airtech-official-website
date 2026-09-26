<script setup lang="ts">
import { adminIdentityApi } from '@/features/identity/services/adminIdentityApi'
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowLeft, LogOut, Save, ShieldAlert } from 'lucide-vue-next'
import DataStatePanel from '@/shared/components/DataStatePanel.vue'
import PageHeader from '@/shared/components/PageHeader.vue'
import { type AdminRoleRecord, type AdminUserRecord } from '@/shared/services/adminApiTypes'
import { apiErrorMessage, apiProblemStatus } from '@/shared/services/cursorPagination'
import { useUiStore } from '@/shared/stores/ui'

const route = useRoute()
const router = useRouter()
const ui = useUiStore()
const user = ref<AdminUserRecord>()
const roles = ref<AdminRoleRecord[]>([])
const managerCandidates = ref<AdminUserRecord[]>([])
const selectedRoles = ref<string[]>([])
const managerUserId = ref('')
const displayName = ref('')
const status = ref<'active' | 'disabled'>('active')
const reason = ref('')
const saving = ref(false)
const state = ref<'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>('loading')
const isSuperAdmin = computed(() => user.value?.roles.includes('super-admin') && user.value.status === 'active')

async function load(): Promise<void> {
  state.value = 'loading'
  try {
    const [recordResult, rolePage, userPage] = await Promise.all([
      adminIdentityApi.getUser(String(route.params.id)),
      adminIdentityApi.listRoles({ limit: 100 }),
      adminIdentityApi.listUsers({ limit: 100, status: 'active' }),
    ])
    const record = recordResult.user
    user.value = record
    roles.value = rolePage.items
    managerCandidates.value = userPage.items.filter((candidate) => candidate.id !== record.id)
    displayName.value = record.displayName
    managerUserId.value = record.managerUserId ?? ''
    selectedRoles.value = [...record.roles]
    status.value = record.status === 'disabled' ? 'disabled' : 'active'
    state.value = 'ready'
  } catch (error) {
    const statusCode = apiProblemStatus(error)
    state.value = statusCode === 403 ? 'forbidden' : statusCode === 404 ? 'empty' : 'error'
  }
}

async function save(): Promise<void> {
  if (!user.value) return
  if (reason.value.trim().length < 10) {
    ui.toast('需要变更原因', '审计原因至少需要 10 个字符。', 'warning')
    return
  }
  try {
    saving.value = true
    const result = await adminIdentityApi.updateUser(user.value.id, user.value.revision, {
      displayName: displayName.value.trim(), status: status.value,
      roleKeys: selectedRoles.value, managerUserId: managerUserId.value || null,
      reason: reason.value.trim(),
    })
    user.value = result.user
    reason.value = ''
    ui.toast('用户已更新', '角色和状态变更已写入审计。')
  } catch (error) {
    ui.toast('更新失败', apiErrorMessage(error, '服务端拒绝了身份变更，请重新加载后重试。'), 'danger')
  } finally {
    saving.value = false
  }
}

async function revokeSessions(): Promise<void> {
  if (!user.value) return
  try {
    await adminIdentityApi.revokeUserSessions(user.value.id)
    ui.toast('会话已撤销', '该用户需要重新登录。')
  } catch (error) {
    ui.toast('撤销失败', apiErrorMessage(error, '请检查身份权限。'), 'danger')
  }
}

onMounted(load)
</script>

<template>
  <div class="page-stack"><PageHeader eyebrow="USER DETAIL" :title="user?.displayName ?? '用户详情'" :description="user?.email ?? '读取身份记录中…'"><template #actions><button class="button button--secondary" type="button" @click="router.push('/users')"><ArrowLeft :size="16" />返回</button><button class="button button--primary" type="button" :disabled="state !== 'ready' || saving" @click="save"><Save :size="16" />{{ saving ? '保存中…' : '保存' }}</button></template></PageHeader><DataStatePanel v-if="state !== 'ready'" :state="state" :title="state === 'empty' ? '用户记录不可用' : ''" @retry="load" /><template v-else-if="user"><div v-if="isSuperAdmin" class="security-baseline"><ShieldAlert :size="18" /><div><strong>高权限 Super Admin</strong><p>变更使用 revision ETag，由服务端执行并发校验、当前账号自停用保护与完整审计；前端状态不构成授权边界。</p></div></div><section class="panel settings-panel"><div class="form-grid"><label class="field"><span>显示名称</span><input v-model="displayName" /></label><label class="field"><span>状态</span><select v-model="status"><option value="active">Active</option><option value="disabled">Disabled</option></select></label><label class="field"><span>直属上级</span><select v-model="managerUserId"><option value="">无直属上级</option><option v-for="candidate in managerCandidates" :key="candidate.id" :value="candidate.id">{{ candidate.displayName }} · {{ candidate.email }}</option></select></label><label class="field"><span>Revision</span><input :value="user.revision" disabled /></label><label class="field"><span>TOTP</span><input :value="user.totpEnabled ? 'Enabled' : 'Not configured'" disabled /></label><label class="field"><span>最近登录</span><input :value="user.lastLoginAt ? new Date(user.lastLoginAt).toLocaleString('zh-CN') : '从未登录'" disabled /></label><label class="field"><span>变更原因</span><textarea v-model="reason" rows="3" placeholder="至少 10 个字符，将写入审计。"></textarea></label></div><fieldset class="role-options"><legend>角色分配</legend><label v-for="role in roles" :key="role.key"><input v-model="selectedRoles" type="checkbox" :value="role.key" /><span><strong>{{ role.displayName }}</strong><small>{{ role.permissions.length }} permissions</small></span></label></fieldset><footer class="danger-actions"><div><strong>撤销全部会话</strong><p>不删除账号或审计历史。</p></div><button class="button button--secondary" type="button" @click="revokeSessions"><LogOut :size="16" />撤销会话</button></footer></section></template></div>
</template>
