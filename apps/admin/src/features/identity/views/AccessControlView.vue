<script setup lang="ts">
import { adminIdentityApi } from '@/features/identity/services/adminIdentityApi'
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { KeyRound, Search, ShieldCheck, UserPlus, UsersRound } from 'lucide-vue-next'
import CursorPaginationControls from '@/shared/components/CursorPaginationControls.vue'
import DataStatePanel from '@/shared/components/DataStatePanel.vue'
import PageHeader from '@/shared/components/PageHeader.vue'
import StatusBadge from '@/shared/components/StatusBadge.vue'
import { useCursorPagination } from '@/shared/composables/useCursorPagination'
import { type AdminRoleRecord, type AdminUserRecord, type UserInvitation } from '@/shared/services/adminApiTypes'
import { apiErrorMessage } from '@/shared/services/cursorPagination'
import { useUiStore } from '@/shared/stores/ui'

const props = defineProps<{ section: 'users' | 'roles' }>()
const route = useRoute()
const router = useRouter()
const ui = useUiStore()
const query = ref('')
const userStatus = ref<'invited' | 'active' | 'disabled' | ''>('')
const userTotal = ref(0)
const roleTotal = ref(0)
const invitations = ref<UserInvitation[]>([])
const invitationResult = ref<UserInvitation>()
const inviteOpen = ref(false)
const inviting = ref(false)
const invite = ref({ displayName: '', email: '', roleKeys: [] as string[] })
let searchTimer: ReturnType<typeof setTimeout> | undefined

const userPager = useCursorPagination<AdminUserRecord>(async (pagination) => {
  const page = await adminIdentityApi.listUsers({ ...pagination, q: query.value || undefined, status: userStatus.value || undefined })
  userTotal.value = page.total
  return page
}, { errorMessage: '用户读取失败。' })
const rolePager = useCursorPagination<AdminRoleRecord>(async (pagination) => {
  const page = await adminIdentityApi.listRoles({
    ...pagination,
    q: props.section === 'roles' ? query.value || undefined : undefined,
  })
  roleTotal.value = page.total
  return page
}, { errorMessage: '角色读取失败。' })
const activePager = computed(() => props.section === 'users' ? userPager : rolePager)
const activeItems = computed(() => activePager.value.items.value)
const state = computed<'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>(() => {
  const pager = activePager.value
  if (pager.loading.value && !activeItems.value.length) return 'loading'
  if (pager.errorStatus.value === 403) return 'forbidden'
  if (pager.error.value) return 'error'
  return activeItems.value.length ? 'ready' : 'empty'
})

function routeString(value: unknown): string { return typeof value === 'string' ? value : '' }
async function loadFromUrl(): Promise<void> {
  query.value = routeString(route.query.q)
  const status = routeString(route.query.status)
  userStatus.value = ['invited', 'active', 'disabled'].includes(status) ? status as typeof userStatus.value : ''
  if (props.section === 'users') {
    const [invitationsPage] = await Promise.all([
      adminIdentityApi.listUserInvitations(), userPager.first(), rolePager.first(),
    ])
    invitations.value = invitationsPage.items
  } else {
    await rolePager.first()
  }
}
function replaceFilters(): void {
  void router.replace({ query: {
    ...(query.value.trim() ? { q: query.value.trim() } : {}),
    ...(props.section === 'users' && userStatus.value ? { status: userStatus.value } : {}),
  } })
}
watch(() => [props.section, route.fullPath], () => void loadFromUrl(), { immediate: true })
watch(userStatus, replaceFilters)
watch(query, () => {
  if (searchTimer !== undefined) clearTimeout(searchTimer)
  searchTimer = setTimeout(replaceFilters, 300)
})
onBeforeUnmount(() => { if (searchTimer !== undefined) clearTimeout(searchTimer) })

async function submitInvite(): Promise<void> {
  inviting.value = true
  try {
    invitationResult.value = await adminIdentityApi.inviteUser({ email: invite.value.email.trim(), displayName: invite.value.displayName.trim(), roleKeys: invite.value.roleKeys })
    inviteOpen.value = false
    invite.value = { displayName: '', email: '', roleKeys: [] }
    ui.toast('邀请已创建', '一次性邀请令牌仅在本次响应中显示，请安全传递。')
    await loadFromUrl()
  } catch (error) { ui.toast('邀请失败', apiErrorMessage(error, '请检查邮箱、角色和权限。'), 'danger') } finally { inviting.value = false }
}
async function revokeInvitation(item: UserInvitation): Promise<void> {
  const reason = window.prompt(`请输入撤销 ${item.email} 邀请的原因：`)?.trim()
  if (!reason || reason.length < 10) return
  try { await adminIdentityApi.revokeInvitation(item.id, reason); await loadFromUrl() }
  catch (error) { ui.toast('撤销失败', apiErrorMessage(error, '请检查邀请状态与权限。'), 'danger') }
}
</script>

<template>
  <div class="page-stack">
    <PageHeader :eyebrow="section === 'users' ? 'IDENTITY' : 'AUTHORIZATION'" :title="section === 'users' ? '用户' : '角色与权限'" :description="section === 'users' ? '服务端搜索、状态筛选和全局总数；管理邀请制后台账号。' : '服务端搜索角色与实际权限组合。'"><template #actions><button v-if="section === 'users'" class="button button--primary" type="button" @click="inviteOpen = true"><UserPlus :size="16" />邀请用户</button></template></PageHeader>
    <div class="security-baseline"><ShieldCheck :size="18" /><div><strong>权限由 Rust API 强制执行</strong><p>菜单隐藏只用于体验；mutation 仍验证会话、CSRF、RBAC、并发与审计上下文。</p></div></div>
    <section v-if="invitationResult?.invitationToken" class="panel settings-panel"><header><h2>一次性邀请令牌</h2></header><p>该令牌不会再次由 API 返回。请通过受控渠道交给 {{ invitationResult.email }}。</p><code class="checksum">{{ invitationResult.invitationToken }}</code></section>
    <section class="panel table-panel"><div class="table-toolbar"><label class="search-field"><Search :size="17" /><input v-model="query" :placeholder="section === 'users' ? '搜索姓名、邮箱或角色' : '搜索角色或权限'" /></label><select v-if="section === 'users'" v-model="userStatus" aria-label="用户状态"><option value="">全部状态</option><option value="invited">Invited</option><option value="active">Active</option><option value="disabled">Disabled</option></select></div>
      <DataStatePanel v-if="state !== 'ready'" :state="state" :title="state === 'empty' ? '没有匹配记录' : state === 'error' ? activePager.error.value || '' : ''" @retry="activePager.refresh" />
      <template v-else-if="section === 'users'"><div class="data-table-wrap"><table class="data-table"><thead><tr><th>用户</th><th>角色</th><th>最近登录</th><th>TOTP</th><th>状态</th></tr></thead><tbody><tr v-for="user in userPager.items.value" :key="user.id"><td><RouterLink class="entity-link" :to="`/users/${user.id}`"><span class="avatar">{{ user.displayName.slice(0, 2).toUpperCase() }}</span><span><strong>{{ user.displayName }}</strong><small>{{ user.email }}</small></span></RouterLink></td><td>{{ user.roles.join(', ') || '未分配' }}</td><td>{{ user.lastLoginAt ? new Date(user.lastLoginAt).toLocaleString('zh-CN') : '从未登录' }}</td><td><StatusBadge :label="user.totpEnabled ? '已启用' : '待配置'" :tone="user.totpEnabled ? 'success' : 'warning'" /></td><td><StatusBadge :label="user.status" :tone="user.status === 'active' ? 'success' : user.status === 'disabled' ? 'neutral' : 'info'" /></td></tr></tbody></table></div><CursorPaginationControls :item-count="userPager.items.value.length" :page-number="userPager.pageNumber.value" :can-previous="userPager.canPrevious.value" :can-next="userPager.canNext.value" :loading="userPager.loading.value" :label="`条用户；服务端匹配总数 ${userTotal}`" @previous="userPager.previous" @next="userPager.next" /></template>
      <template v-else><div class="role-grid"><RouterLink v-for="role in rolePager.items.value" :key="role.id" class="panel role-card" :to="`/roles/${role.id}`"><header><span><UsersRound :size="18" /></span><StatusBadge :label="role.systemRole ? 'System' : 'Custom'" :tone="role.systemRole ? 'warning' : 'neutral'" /></header><h2>{{ role.displayName }}</h2><p><code>{{ role.key }}</code></p><footer><span>{{ role.permissions.length }} 项权限</span><strong>查看权限</strong></footer></RouterLink></div><CursorPaginationControls :item-count="rolePager.items.value.length" :page-number="rolePager.pageNumber.value" :can-previous="rolePager.canPrevious.value" :can-next="rolePager.canNext.value" :loading="rolePager.loading.value" :label="`条角色；服务端匹配总数 ${roleTotal}`" @previous="rolePager.previous" @next="rolePager.next" /></template>
    </section>
    <section v-if="section === 'users' && invitations.length" class="panel table-panel"><header class="panel__header table-panel__header"><div><p class="eyebrow">INVITATIONS</p><h2>邀请记录</h2></div></header><div class="data-table-wrap"><table class="data-table"><thead><tr><th>受邀人</th><th>角色</th><th>创建时间</th><th>有效期</th><th>状态</th><th>操作</th></tr></thead><tbody><tr v-for="item in invitations" :key="item.id"><td><strong>{{ item.displayName }}</strong><br /><small>{{ item.email }}</small></td><td>{{ item.roleKeys.join(', ') }}</td><td>{{ new Date(item.invitedAt).toLocaleString('zh-CN') }}</td><td>{{ new Date(item.expiresAt).toLocaleString('zh-CN') }}</td><td><StatusBadge :label="item.status" :tone="item.status === 'pending' ? 'info' : 'neutral'" /></td><td><button v-if="item.status === 'pending'" class="button button--quiet" type="button" @click="revokeInvitation(item)">撤销</button><span v-else>—</span></td></tr></tbody></table></div></section>
    <div v-if="inviteOpen" class="modal-backdrop" @click.self="inviteOpen = false"><form class="modal-card" @submit.prevent="submitInvite"><header><div><p class="eyebrow">INVITE</p><h2>邀请后台用户</h2></div><KeyRound :size="20" /></header><label class="field"><span>显示名称</span><input v-model="invite.displayName" required /></label><label class="field"><span>邮箱</span><input v-model="invite.email" type="email" required /></label><fieldset class="role-options"><legend>角色</legend><label v-for="role in rolePager.items.value" :key="role.key"><input v-model="invite.roleKeys" type="checkbox" :value="role.key" /><span><strong>{{ role.displayName }}</strong><small>{{ role.permissions.length }} permissions</small></span></label></fieldset><footer><button class="button button--quiet" type="button" @click="inviteOpen = false">取消</button><button class="button button--primary" type="submit" :disabled="inviting || !invite.roleKeys.length">{{ inviting ? '正在创建…' : '发送邀请' }}</button></footer></form></div>
  </div>
</template>
