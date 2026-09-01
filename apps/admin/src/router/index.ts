import { createRouter, createWebHistory, type RouteRecordRaw } from 'vue-router'
import { devtoolsRoutes } from 'virtual:devtools-routes'
import { useAuthStore } from '@/stores/auth'
import type { Permission } from '@/types/domain'

const workspaceChildren: RouteRecordRaw[] = [
  {
    path: '',
    name: 'dashboard',
    component: () => import('@/views/DashboardView.vue'),
    meta: { title: '工作台', requiresAuth: true, permission: 'dashboard.read' },
  },
  {
    path: 'content',
    name: 'content',
    component: () => import('@/views/ContentListView.vue'),
    meta: { title: '内容中心', requiresAuth: true, permission: 'content.read' },
  },
  {
    path: 'content/:id/edit',
    name: 'content-editor',
    component: () => import('@/views/ContentEditorView.vue'),
    meta: { title: '内容编辑器', requiresAuth: true, permission: 'content.write' },
  },
  {
    path: 'products',
    name: 'products',
    component: () => import('@/views/ProductsView.vue'),
    meta: { title: '产品中心', requiresAuth: true, permission: 'product.read' },
  },
  {
    path: 'products/:id',
    name: 'product-detail',
    component: () => import('@/views/ProductDetailView.vue'),
    meta: { title: '产品详情', requiresAuth: true, permission: 'product.read' },
  },
  {
    path: 'integrations/feishu',
    name: 'feishu-sync',
    component: () => import('@/views/FeishuSyncView.vue'),
    meta: { title: 'Feishu 同步', requiresAuth: true, permission: 'integration.run' },
  },
  {
    path: 'media',
    name: 'media',
    component: () => import('@/views/MediaView.vue'),
    meta: { title: '媒体中心', requiresAuth: true, permission: 'media.write' },
  },
  {
    path: 'rfqs',
    name: 'rfqs',
    component: () => import('@/views/InboxView.vue'),
    props: { kind: 'rfq' },
    meta: { title: 'RFQ 收件箱', requiresAuth: true, permission: 'rfq.read' },
  },
  {
    path: 'contacts',
    name: 'contacts',
    component: () => import('@/views/InboxView.vue'),
    props: { kind: 'contact' },
    meta: { title: 'Contact', requiresAuth: true, permission: 'rfq.read' },
  },
  {
    path: 'analytics',
    name: 'analytics',
    component: () => import('@/views/AnalyticsView.vue'),
    meta: { title: 'Analytics', requiresAuth: true, permission: 'analytics.read' },
  },
  {
    path: 'users',
    name: 'users',
    component: () => import('@/views/AccessControlView.vue'),
    props: { section: 'users' },
    meta: { title: '用户', requiresAuth: true, permission: 'identity.manage' },
  },
  {
    path: 'roles',
    name: 'roles',
    component: () => import('@/views/AccessControlView.vue'),
    props: { section: 'roles' },
    meta: { title: '角色与权限', requiresAuth: true, permission: 'identity.manage' },
  },
  {
    path: 'audit',
    name: 'audit',
    component: () => import('@/views/AuditView.vue'),
    meta: { title: '审计日志', requiresAuth: true, permission: 'audit.read' },
  },
  {
    path: 'account/security',
    name: 'account-security',
    component: () => import('@/views/SettingsView.vue'),
    meta: { title: '安全与会话', requiresAuth: true },
  },
  {
    path: 'settings/:section?',
    name: 'settings',
    component: () => import('@/views/SettingsView.vue'),
    meta: { title: '系统设置', requiresAuth: true, permission: 'settings.manage' },
  },
  {
    path: 'operations',
    name: 'operations',
    component: () => import('@/views/OperationsView.vue'),
    meta: { title: '运维任务', requiresAuth: true, permission: 'operations.run' },
  },
  ...devtoolsRoutes,
  {
    path: ':pathMatch(.*)*',
    name: 'not-found',
    component: () => import('@/views/NotFoundView.vue'),
    meta: { title: '页面不存在', requiresAuth: true },
  },
]

const router = createRouter({
  history: createWebHistory(),
  scrollBehavior: () => ({ top: 0 }),
  routes: [
    {
      path: '/login',
      name: 'login',
      component: () => import('@/views/LoginView.vue'),
      meta: { title: '登录' },
    },
    {
      path: '/setup',
      name: 'setup',
      component: () => import('@/views/SetupView.vue'),
      meta: { title: '初始化平台' },
    },
    {
      path: '/',
      component: () => import('@/layouts/AdminShell.vue'),
      children: workspaceChildren,
    },
  ],
})

router.beforeEach(async (to) => {
  document.title = `${String(to.meta.title ?? '管理平台')} · AIRTEKPOWER`
  const auth = useAuthStore()

  if (!auth.initialized) {
    try {
      await auth.initialize()
    } catch {
      // A network error must never create a local authenticated session.
    }
  }

  if (to.meta.requiresAuth && !auth.isAuthenticated) {
    return { name: 'login', query: { redirect: to.fullPath } }
  }

  if ((to.name === 'login' || to.name === 'setup') && auth.isAuthenticated) {
    return { name: 'dashboard' }
  }

  const permission = to.meta.permission as Permission | undefined
  if (permission && !auth.hasPermission(permission)) {
    return { name: 'dashboard', query: { denied: permission } }
  }

  return true
})

export default router
