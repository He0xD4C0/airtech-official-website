<script setup lang="ts">
import { computed } from 'vue'
import { KeyRound, MoreHorizontal, ShieldCheck, UserPlus, UsersRound } from 'lucide-vue-next'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { mockApiEnabled } from '@/services/adminApi'
import { useUiStore } from '@/stores/ui'

const props = defineProps<{ section: 'users' | 'roles' }>()
const isUsers = computed(() => props.section === 'users')
const ui = useUiStore()

const demoUsers = [
  ['Demo Super Admin', 'demo-admin@localhost.invalid', 'Super Admin · Demo', '开发会话', 'active'],
  ['Demo Content Editor', 'demo-editor@localhost.invalid', 'Content Editor · Demo', '从未登录', 'invited'],
  ['Demo Analyst', 'demo-analyst@localhost.invalid', 'Analyst · Demo', '从未登录', 'disabled'],
]
const users = mockApiEnabled ? demoUsers : []

const roles = [
  ['Super Admin', '系统与业务全部权限', '1', 'Protected'],
  ['Content Editor', '编辑内容，不直接发布', '1', 'Customizable'],
  ['Publisher', '发布、计划发布与回滚', '0', 'Customizable'],
  ['Product Manager', '产品、来源冲突与发布', '0', 'Customizable'],
  ['RFQ Operator', '查看与分配授权范围内询盘', '0', 'Customizable'],
  ['Auditor', '只读审计与任务记录', '0', 'Customizable'],
]

function unavailable(): void {
  ui.toast(mockApiEnabled ? '开发演示操作' : '身份管理 API 尚未启用', mockApiEnabled ? '不会创建真实账号或角色。' : '当前界面不会伪造身份变更。', 'warning')
}
</script>

<template>
  <div class="page-stack">
    <PageHeader :eyebrow="isUsers ? 'IDENTITY' : 'AUTHORIZATION'" :title="isUsers ? '用户' : '角色与权限'" :description="isUsers ? '管理邀请制本地账号、TOTP、会话与停用状态。' : '通过可组合权限控制编辑、发布、产品、询盘、分析与高风险操作。'">
      <template #actions><button class="button button--primary" type="button" @click="unavailable"><UserPlus v-if="isUsers" :size="16" /><KeyRound v-else :size="16" />{{ isUsers ? '邀请用户' : '新建角色' }}</button></template>
    </PageHeader>

    <div class="security-baseline"><ShieldCheck :size="18" /><div><strong>权限由 Rust API 强制执行</strong><p>后台菜单隐藏仅改善体验；每个 mutation 都会重新验证权限、CSRF、会话和资源范围。</p></div></div>

    <section v-if="isUsers" class="panel table-panel">
      <div class="data-table-wrap"><table class="data-table"><thead><tr><th>用户</th><th>角色</th><th>最近活动</th><th>TOTP</th><th>状态</th><th><span class="sr-only">操作</span></th></tr></thead><tbody><tr v-for="user in users" :key="user[1]"><td><div class="user-cell"><span>{{ user[0].slice(0, 2).toUpperCase() }}</span><div><strong>{{ user[0] }}</strong><small>{{ user[1] }}</small></div></div></td><td>{{ user[2] }}</td><td>{{ user[3] }}</td><td><StatusBadge :label="user[4] === 'active' ? '已启用' : '待配置'" :tone="user[4] === 'active' ? 'success' : 'warning'" /></td><td><StatusBadge :label="user[4] === 'disabled' ? '已停用' : user[4] === 'invited' ? '已邀请' : '活跃'" :tone="user[4] === 'disabled' ? 'neutral' : user[4] === 'invited' ? 'info' : 'success'" /></td><td><button type="button" class="icon-button" aria-label="用户操作" @click="unavailable"><MoreHorizontal :size="18" /></button></td></tr><tr v-if="!users.length"><td colspan="6"><div class="module-empty"><UsersRound :size="27" /><h2>身份管理 API 尚未配置</h2><p>生产界面不会显示演示账号、邮箱或会话状态。</p></div></td></tr></tbody></table></div>
    </section>

    <section v-else class="role-grid">
      <article v-for="role in roles" :key="role[0]" class="panel role-card"><header><span><UsersRound :size="18" /></span><StatusBadge :label="role[3]" :tone="role[3] === 'Protected' ? 'warning' : 'neutral'" /></header><h2>{{ role[0] }}</h2><p>{{ role[1] }}</p><footer><span>{{ mockApiEnabled ? `${role[2]} 位演示用户` : '默认角色模板 · 未读取成员数' }}</span><button type="button" @click="unavailable">查看权限</button></footer></article>
    </section>

    <p class="safe-demo-footer">{{ mockApiEnabled ? '所有账号、角色数量与邮箱均为本地开发演示信息。' : '角色卡片为计划中的默认模板；用户、成员数与权限组合尚未由身份 API 载入。' }}</p>
  </div>
</template>
