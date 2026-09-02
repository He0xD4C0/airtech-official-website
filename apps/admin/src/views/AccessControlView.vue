<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { KeyRound, ShieldCheck, UserPlus, UsersRound } from 'lucide-vue-next'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { adminApi, type AdminRoleRecord, type AdminUserRecord, type UserInvitation } from '@/services/adminApi'
import { apiErrorMessage, apiProblemStatus, collectCursorPages } from '@/services/cursorPagination'
import { useUiStore } from '@/stores/ui'

const props = defineProps<{ section: 'users' | 'roles' }>()
const ui = useUiStore()
const isUsers = computed(() => props.section === 'users')
const users = ref<AdminUserRecord[]>([])
const roles = ref<AdminRoleRecord[]>([])
const invitations = ref<UserInvitation[]>([])
const invitationResult = ref<UserInvitation>()
const state = ref<'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>('loading')
const inviteOpen = ref(false)
const inviting = ref(false)
const invite = ref({ displayName: '', email: '', roleKeys: [] as string[] })

async function load(): Promise<void> {
  state.value = 'loading'
  try {
    const [allUsers, rolePage, invitationPage] = await Promise.all([
      collectCursorPages((pagination) => adminApi.listUsers(pagination)),
      adminApi.listRoles({ limit: 100 }),
      adminApi.listUserInvitations(),
    ])
    users.value = allUsers
    roles.value = rolePage.items
    invitations.value = invitationPage.items
    state.value = (isUsers.value ? users.value.length : roles.value.length) ? 'ready' : 'empty'
  } catch (error) {
    state.value = apiProblemStatus(error) === 403 ? 'forbidden' : 'error'
  }
}

async function submitInvite(): Promise<void> {
  inviting.value = true
  try {
    invitationResult.value = await adminApi.inviteUser({ email: invite.value.email.trim(), displayName: invite.value.displayName.trim(), roleKeys: invite.value.roleKeys })
    inviteOpen.value = false
    invite.value = { displayName: '', email: '', roleKeys: [] }
    ui.toast('邀请已创建', '一次性邀请令牌仅在本次响应中显示，请安全传递。')
    await load()
  } catch (error) {
    ui.toast('邀请失败', apiErrorMessage(error, '请检查邮箱、角色和权限。'), 'danger')
  } finally {
    inviting.value = false
  }
}

async function revokeInvitation(item: UserInvitation): Promise<void> {
  const reason = window.prompt(`请输入撤销 ${item.email} 邀请的原因：`)?.trim()
  if (!reason) return
  if (reason.length < 10) {
    ui.toast('撤销原因过短', '审计原因至少需要 10 个字符。', 'warning')
    return
  }
  try {
    await adminApi.revokeInvitation(item.id, reason)
    ui.toast('邀请已撤销', '一次性令牌已失效，审计记录已保存。')
    await load()
  } catch (error) {
    ui.toast('撤销失败', apiErrorMessage(error, '请检查邀请状态与权限。'), 'danger')
  }
}

onMounted(load)
</script>

<template>
  <div class="page-stack">
    <PageHeader :eyebrow="isUsers ? 'IDENTITY' : 'AUTHORIZATION'" :title="isUsers ? '用户' : '角色与权限'" :description="isUsers ? '管理邀请制后台账号、状态、TOTP 与会话。' : '查看默认角色和服务端实际权限组合。'">
      <template #actions><button v-if="isUsers" class="button button--primary" type="button" @click="inviteOpen = true"><UserPlus :size="16" />邀请用户</button></template>
    </PageHeader>
    <div class="security-baseline"><ShieldCheck :size="18" /><div><strong>权限由 Rust API 强制执行</strong><p>菜单隐藏只用于体验；所有 mutation 均验证会话、CSRF、RBAC 和审计上下文。</p></div></div>
    <section v-if="invitationResult?.invitationToken" class="panel settings-panel"><header><h2>一次性邀请令牌</h2></header><p>该令牌不会再次由 API 返回。请通过受控渠道交给 {{ invitationResult.email }}。</p><code class="checksum">{{ invitationResult.invitationToken }}</code></section>
    <DataStatePanel v-if="state !== 'ready'" :state="state" :title="state === 'empty' ? (isUsers ? '暂无用户' : '暂无角色') : ''" @retry="load" />
    <template v-else-if="isUsers"><section class="panel table-panel"><div class="data-table-wrap"><table class="data-table"><thead><tr><th>用户</th><th>角色</th><th>最近登录</th><th>TOTP</th><th>状态</th></tr></thead><tbody><tr v-for="user in users" :key="user.id"><td><RouterLink class="entity-link" :to="`/users/${user.id}`"><span class="avatar">{{ user.displayName.slice(0, 2).toUpperCase() }}</span><span><strong>{{ user.displayName }}</strong><small>{{ user.email }}</small></span></RouterLink></td><td>{{ user.roles.join(', ') || '未分配' }}</td><td>{{ user.lastLoginAt ? new Date(user.lastLoginAt).toLocaleString('zh-CN') : '从未登录' }}</td><td><StatusBadge :label="user.totpEnabled ? '已启用' : '待配置'" :tone="user.totpEnabled ? 'success' : 'warning'" /></td><td><StatusBadge :label="user.status" :tone="user.status === 'active' ? 'success' : user.status === 'disabled' ? 'neutral' : 'info'" /></td></tr></tbody></table></div></section><section v-if="invitations.length" class="panel table-panel"><header class="panel__header table-panel__header"><div><p class="eyebrow">INVITATIONS</p><h2>邀请记录</h2></div></header><div class="data-table-wrap"><table class="data-table"><thead><tr><th>受邀人</th><th>角色</th><th>创建时间</th><th>有效期</th><th>状态</th><th>操作</th></tr></thead><tbody><tr v-for="item in invitations" :key="item.id"><td><strong>{{ item.displayName }}</strong><br /><small>{{ item.email }}</small></td><td>{{ item.roleKeys.join(', ') }}</td><td>{{ new Date(item.invitedAt).toLocaleString('zh-CN') }}</td><td>{{ new Date(item.expiresAt).toLocaleString('zh-CN') }}</td><td><StatusBadge :label="item.status" :tone="item.status === 'pending' ? 'info' : 'neutral'" /></td><td><button v-if="item.status === 'pending'" class="button button--quiet" type="button" @click="revokeInvitation(item)">撤销</button><span v-else>—</span></td></tr></tbody></table></div></section></template>
    <section v-else class="role-grid"><RouterLink v-for="role in roles" :key="role.id" class="panel role-card" :to="`/roles/${role.id}`"><header><span><UsersRound :size="18" /></span><StatusBadge :label="role.systemRole ? 'System' : 'Custom'" :tone="role.systemRole ? 'warning' : 'neutral'" /></header><h2>{{ role.displayName }}</h2><p><code>{{ role.key }}</code></p><footer><span>{{ role.permissions.length }} 项权限</span><strong>查看权限</strong></footer></RouterLink></section>
    <div v-if="inviteOpen" class="modal-backdrop" @click.self="inviteOpen = false"><form class="modal-card" @submit.prevent="submitInvite"><header><div><p class="eyebrow">INVITE</p><h2>邀请后台用户</h2></div><KeyRound :size="20" /></header><label class="field"><span>显示名称</span><input v-model="invite.displayName" required /></label><label class="field"><span>邮箱</span><input v-model="invite.email" type="email" required /></label><fieldset class="role-options"><legend>角色</legend><label v-for="role in roles" :key="role.key"><input v-model="invite.roleKeys" type="checkbox" :value="role.key" /><span><strong>{{ role.displayName }}</strong><small>{{ role.permissions.length }} permissions</small></span></label></fieldset><footer><button class="button button--quiet" type="button" @click="inviteOpen = false">取消</button><button class="button button--primary" type="submit" :disabled="inviting || !invite.roleKeys.length">{{ inviting ? '正在创建…' : '发送邀请' }}</button></footer></form></div>
  </div>
</template>
