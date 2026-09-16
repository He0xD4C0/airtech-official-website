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
    path: 'content/drafts',
    name: 'content-drafts',
    component: () => import('@/views/ContentListView.vue'),
    meta: { title: '内容中心', requiresAuth: true, permission: 'content.read' },
  },
  {
    path: 'content/drafts/new',
    name: 'content-drafts-new',
    component: () => import('@/views/ContentCreateView.vue'),
    meta: { title: '新建内容', requiresAuth: true, permission: 'content.write' },
  },
  {
    path: 'content/drafts/:draftId',
    name: 'content-draft-editor',
    component: () => import('@/views/ContentEditorView.vue'),
    meta: { title: '内容编辑器', requiresAuth: true, permissionsAny: ['content.write', 'content.publish'] },
  },
  {
    path: 'content/reviews',
    name: 'content-reviews',
    component: () => import('@/views/ContentReviewsView.vue'),
    meta: { title: '内容审核', requiresAuth: true, permission: 'content.publish' },
  },
  {
    path: 'content/published',
    name: 'content-published',
    component: () => import('@/views/PublishedContentView.vue'),
    meta: { title: '已发布内容', requiresAuth: true, permission: 'content.read' },
  },
  {
    path: 'content/published/:contentId',
    name: 'content-published-detail',
    component: () => import('@/views/PublishedContentDetailView.vue'),
    meta: { title: '已发布内容详情', requiresAuth: true, permission: 'content.read' },
  },
  {
    path: 'site/general-information',
    name: 'site-general-information',
    component: () => import('@/views/SiteSingletonView.vue'),
    props: { section: 'general-information' },
    meta: { title: 'General Information', requiresAuth: true, permission: 'content.write' },
  },
  {
    path: 'site/navigation',
    name: 'site-navigation',
    component: () => import('@/views/SiteSingletonView.vue'),
    props: { section: 'navigation' },
    meta: { title: 'Navigation', requiresAuth: true, permission: 'content.write' },
  },
  {
    path: 'site/footer',
    name: 'site-footer',
    component: () => import('@/views/SiteSingletonView.vue'),
    props: { section: 'footer' },
    meta: { title: 'Footer', requiresAuth: true, permission: 'content.write' },
  },
  {
    path: 'products',
    name: 'products',
    component: () => import('@/views/ProductsView.vue'),
    meta: { title: '产品中心', requiresAuth: true, permission: 'product.read' },
  },
  {
    path: 'products/imports',
    name: 'product-imports',
    component: () => import('@/views/ProductImportsView.vue'),
    meta: { title: '产品数据导入', requiresAuth: true, permission: 'product.write' },
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
    meta: {
      title: '媒体中心',
      requiresAuth: true,
      permission: 'media.write',
    },
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
    path: 'analytics/sources',
    name: 'analytics-sources',
    component: () => import('@/views/GuestAnalyticsView.vue'),
    meta: { title: '站外来源', requiresAuth: true, permission: 'analytics.read' },
  },
  {
    path: 'users',
    name: 'users',
    component: () => import('@/views/AccessControlView.vue'),
    props: { section: 'users' },
    meta: { title: '用户', requiresAuth: true, permission: 'identity.manage' },
  },
  {
    path: 'users/:id',
    name: 'user-detail',
    component: () => import('@/views/UserDetailView.vue'),
    meta: { title: '用户详情', requiresAuth: true, permission: 'identity.manage' },
  },
  {
    path: 'roles',
    name: 'roles',
    component: () => import('@/views/AccessControlView.vue'),
    props: { section: 'roles' },
    meta: { title: '角色与权限', requiresAuth: true, permission: 'identity.manage' },
  },
  {
    path: 'roles/:id',
    name: 'role-detail',
    component: () => import('@/views/RoleDetailView.vue'),
    meta: { title: '角色详情', requiresAuth: true, permission: 'identity.manage' },
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
      path: '/accept-invitation',
      name: 'accept-invitation',
      component: () => import('@/views/AcceptInvitationView.vue'),
      meta: { title: '接受邀请' },
    },
    {
      path: '/forbidden',
      name: 'forbidden',
      component: () => import('@/views/ForbiddenView.vue'),
      meta: { title: '没有访问权限', requiresAuth: true },
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

  // Mount the invitation page before any session request so its component can
  // replace the token-bearing URL immediately. The token is never persisted.
  if (to.name === 'accept-invitation') {
    return auth.initialized && auth.isAuthenticated ? { name: 'dashboard' } : true
  }

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

  if (auth.requiresTotpEnrollment && to.name !== 'account-security') {
    return { name: 'account-security' }
  }

  if ((to.name === 'login' || to.name === 'setup') && auth.isAuthenticated) {
    return { name: 'dashboard' }
  }

  const permission = to.meta.permission as Permission | undefined
  if (permission && !auth.hasPermission(permission)) {
    return { name: 'forbidden', query: { permission, from: to.fullPath } }
  }

  const permissionsAny = to.meta.permissionsAny as readonly Permission[] | undefined
  if (permissionsAny?.length && !permissionsAny.some((candidate) => auth.hasPermission(candidate))) {
    return { name: 'forbidden', query: { permission: permissionsAny.join('|'), from: to.fullPath } }
  }

  return true
})

export default router
