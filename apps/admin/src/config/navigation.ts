import type { Component } from 'vue'
import {
  Activity,
  Boxes,
  Cable,
  ChartNoAxesCombined,
  CircleGauge,
  ContactRound,
  FileText,
  FolderKanban,
  History,
  Inbox,
  KeyRound,
  Library,
  Settings,
  ShieldCheck,
  SlidersHorizontal,
  UsersRound,
  Wrench,
} from 'lucide-vue-next'
import { devtoolsNavigation } from 'virtual:devtools-routes'
import type { Permission } from '@/types/domain'

export interface NavItem {
  label: string
  to: string
  icon: Component
  permission?: Permission
  badge?: string
  devOnly?: boolean
}

export interface NavGroup {
  label: string
  items: NavItem[]
}

export const navigation: NavGroup[] = [
  {
    label: '概览',
    items: [{ label: '工作台', to: '/', icon: CircleGauge, permission: 'dashboard.read' }],
  },
  {
    label: '内容与产品',
    items: [
      { label: '内容中心', to: '/content', icon: FileText, permission: 'content.read' },
      { label: '产品中心', to: '/products', icon: Boxes, permission: 'product.read' },
      { label: 'Feishu 同步', to: '/integrations/feishu', icon: Cable, permission: 'integration.run', badge: '2' },
      { label: '媒体中心', to: '/media', icon: Library, permission: 'media.write' },
    ],
  },
  {
    label: '业务',
    items: [
      { label: 'RFQ 收件箱', to: '/rfqs', icon: Inbox, permission: 'rfq.read', badge: '4' },
      { label: 'Contact', to: '/contacts', icon: ContactRound, permission: 'rfq.read' },
      { label: 'Analytics', to: '/analytics', icon: ChartNoAxesCombined, permission: 'analytics.read' },
    ],
  },
  {
    label: '系统',
    items: [
      { label: '用户', to: '/users', icon: UsersRound, permission: 'identity.manage' },
      { label: '角色与权限', to: '/roles', icon: KeyRound, permission: 'identity.manage' },
      { label: '审计日志', to: '/audit', icon: History, permission: 'audit.read' },
      { label: '系统设置', to: '/settings', icon: Settings, permission: 'settings.manage' },
      { label: '运维任务', to: '/operations', icon: Wrench, permission: 'operations.run' },
      ...devtoolsNavigation,
    ],
  },
]

export const quickActions = [
  { label: '新建文章', to: '/content/new/edit', icon: FileText },
  { label: '查看同步差异', to: '/integrations/feishu', icon: SlidersHorizontal },
  { label: '处理新 RFQ', to: '/rfqs', icon: FolderKanban },
  { label: '检查任务', to: '/operations', icon: Activity },
  { label: '安全设置', to: '/account/security', icon: ShieldCheck },
]
