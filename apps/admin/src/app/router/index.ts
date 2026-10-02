import { createRouter, createWebHistory, type RouteRecordRaw } from 'vue-router'
import { devtoolsRoutes } from 'virtual:devtools-routes'
import { useAuthStore } from '@/shared/stores/auth'
import type { Permission } from '@/shared/types/domain'

const workspaceChildren: RouteRecordRaw[] = [
  {
    path: '',
    name: 'dashboard',
    component: () => import('@/features/dashboard/views/DashboardView.vue'),
    meta: { title: '工作台', requiresAuth: true, permission: 'dashboard.read' },
  },
  {
    path: 'content/drafts',
    name: 'content-drafts',
    component: () => import('@/features/content/views/ContentListView.vue'),
    meta: { title: '内容中心', requiresAuth: true, permission: 'content.read' },
  },
  {
    path: 'content/drafts/new',
    name: 'content-drafts-new',
    component: () => import('@/features/content/views/ContentCreateView.vue'),
    meta: { title: '新建内容', requiresAuth: true, permission: 'content.write' },
  },
  {
    path: 'content/drafts/:draftId',
    name: 'content-draft-editor',
    component: () => import('@/features/content/views/ContentEditorView.vue'),
    meta: { title: '内容编辑器', requiresAuth: true, permissionsAny: ['content.write', 'content.publish'] },
  },
  {
    path: 'content/reviews',
    name: 'content-reviews',
    component: () => import('@/features/content/views/ContentReviewsView.vue'),
    meta: { title: '内容审核', requiresAuth: true, permission: 'content.publish' },
  },
  {
    path: 'content/published',
    name: 'content-published',
    component: () => import('@/features/content/views/PublishedContentView.vue'),
    meta: { title: '已发布内容', requiresAuth: true, permission: 'content.read' },
  },
  {
    path: 'content/published/:contentId',
    name: 'content-published-detail',
    component: () => import('@/features/content/views/PublishedContentDetailView.vue'),
    meta: { title: '已发布内容详情', requiresAuth: true, permission: 'content.read' },
  },
  {
    path: 'site/general-information',
    name: 'site-general-information',
    component: () => import('@/features/content/views/SiteSingletonView.vue'),
    props: { section: 'general-information' },
    meta: { title: 'General Information', requiresAuth: true, permission: 'content.write' },
  },
  {
    path: 'site/navigation',
    name: 'site-navigation',
    component: () => import('@/features/content/views/SiteSingletonView.vue'),
    props: { section: 'navigation' },
    meta: { title: 'Navigation', requiresAuth: true, permission: 'content.write' },
  },
  {
    path: 'site/footer',
    name: 'site-footer',
    component: () => import('@/features/content/views/SiteSingletonView.vue'),
    props: { section: 'footer' },
    meta: { title: 'Footer', requiresAuth: true, permission: 'content.write' },
  },
  {
    path: 'products',
    name: 'products',
    component: () => import('@/features/catalog/views/ProductsView.vue'),
    meta: { title: '产品中心', requiresAuth: true, permission: 'product.read' },
  },
  {
    path: 'products/imports',
    name: 'product-imports',
    component: () => import('@/features/catalog/views/ProductImportsView.vue'),
    meta: { title: '产品数据导入', requiresAuth: true, permission: 'product.write' },
  },
  {
    path: 'products/:id',
    name: 'product-detail',
    component: () => import('@/features/catalog/views/ProductDetailView.vue'),
    meta: { title: '产品详情', requiresAuth: true, permission: 'product.read' },
  },
  {
    path: 'integrations/feishu',
    name: 'feishu-sync',
    component: () => import('@/features/integrations/views/FeishuSyncView.vue'),
    meta: { title: 'Feishu 同步', requiresAuth: true, permission: 'integration.run' },
  },
  {
    path: 'media',
    name: 'media',
    component: () => import('@/features/media/views/MediaView.vue'),
    meta: {
      title: '媒体中心',
      requiresAuth: true,
      permission: 'media.write',
    },
  },
  {
    path: 'rfqs',
    name: 'rfqs',
    component: () => import('@/features/inbox/views/InboxView.vue'),
    props: { kind: 'rfq' },
    meta: { title: 'RFQ 收件箱', requiresAuth: true, permission: 'rfq.read' },
  },
  {
    path: 'contacts',
    name: 'contacts',
    component: () => import('@/features/inbox/views/InboxView.vue'),
    props: { kind: 'contact' },
    meta: { title: 'Contact', requiresAuth: true, permission: 'rfq.read' },
  },
  {
    path: 'analytics',
    name: 'analytics',
    component: () => import('@/features/analytics/views/AnalyticsView.vue'),
    meta: { title: 'Analytics', requiresAuth: true, permission: 'analytics.read' },
  },
  {
    path: 'analytics/sources',
    name: 'analytics-sources',
    component: () => import('@/features/analytics/views/GuestAnalyticsView.vue'),
    meta: { title: '站外来源', requiresAuth: true, permission: 'analytics.read' },
  },
  {
    path: 'users',
    name: 'users',
    component: () => import('@/features/identity/views/AccessControlView.vue'),
    props: { section: 'users' },
    meta: { title: '用户', requiresAuth: true, permission: 'identity.manage' },
  },
  {
    path: 'users/:id',
    name: 'user-detail',
    component: () => import('@/features/identity/views/UserDetailView.vue'),
    meta: { title: '用户详情', requiresAuth: true, permission: 'identity.manage' },
  },
  {
    path: 'roles',
    name: 'roles',
    component: () => import('@/features/identity/views/AccessControlView.vue'),
    props: { section: 'roles' },
    meta: { title: '角色与权限', requiresAuth: true, permission: 'identity.manage' },
  },
  {
    path: 'roles/:id',
    name: 'role-detail',
    component: () => import('@/features/identity/views/RoleDetailView.vue'),
    meta: { title: '角色详情', requiresAuth: true, permission: 'identity.manage' },
  },
  {
    path: 'audit',
    name: 'audit',
    component: () => import('@/features/audit/views/AuditView.vue'),
    meta: { title: '审计日志', requiresAuth: true, permission: 'audit.read' },
  },
  {
    path: 'account/security',
    name: 'account-security',
    component: () => import('@/features/settings/views/SettingsView.vue'),
    meta: { title: '安全与会话', requiresAuth: true },
  },
  {
    path: 'settings/:section?',
    name: 'settings',
    component: () => import('@/features/settings/views/SettingsView.vue'),
    meta: { title: '系统设置', requiresAuth: true, permission: 'settings.manage' },
  },
  ...devtoolsRoutes,
  {
    path: ':pathMatch(.*)*',
    name: 'not-found',
    component: () => import('@/app/views/NotFoundView.vue'),
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
      component: () => import('@/features/auth/views/LoginView.vue'),
      meta: { title: '登录' },
    },
    {
      path: '/setup',
      name: 'setup',
      component: () => import('@/features/auth/views/SetupView.vue'),
      meta: { title: '初始化平台' },
    },
    {
      path: '/accept-invitation',
      name: 'accept-invitation',
      component: () => import('@/features/auth/views/AcceptInvitationView.vue'),
      meta: { title: '接受邀请' },
    },
    {
      path: '/onboarding',
      name: 'onboarding',
      component: () => import('@/features/auth/views/OnboardingView.vue'),
      meta: { title: '首次安全设置', requiresAuth: true },
    },
    {
      path: '/forbidden',
      name: 'forbidden',
      component: () => import('@/app/views/ForbiddenView.vue'),
      meta: { title: '没有访问权限', requiresAuth: true },
    },
    {
      path: '/',
      component: () => import('@/app/layouts/AdminShell.vue'),
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

  if (auth.requiresOnboarding && to.name !== 'onboarding') {
    return { name: 'onboarding' }
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
