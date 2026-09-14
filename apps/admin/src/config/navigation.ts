import type { Component } from 'vue'
import {
  Building2,
  Boxes,
  Cable,
  ChartNoAxesCombined,
  CircleGauge,
  ContactRound,
  FileText,
  FileUp,
  FolderKanban,
  History,
  Inbox,
  KeyRound,
  Library,
  Newspaper,
  Settings,
  ShieldCheck,
  SlidersHorizontal,
  UsersRound,
  Waypoints,
} from 'lucide-vue-next'
import { devtoolsNavigation } from 'virtual:devtools-routes'
import type { Permission } from '@/types/domain'

export interface NavItem {
  label: string
  to: string
  icon: Component
  permission?: Permission
  permissionsAny?: readonly Permission[]
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
    label: '网站数据',
    items: [
      { label: '内容中心', to: '/content', icon: FileText, permission: 'content.read' },
      { label: '新闻', to: '/content?kind=news', icon: Newspaper, permission: 'content.read' },
      { label: 'General Information', to: '/site/general-information', icon: Building2, permission: 'content.write' },
      { label: 'Navigation', to: '/site/navigation', icon: Waypoints, permission: 'content.write' },
      { label: 'Footer', to: '/site/footer', icon: SlidersHorizontal, permission: 'content.write' },
      {
        label: '媒体中心',
        to: '/media',
        icon: Library,
        permission: 'media.write',
      },
    ],
  },
  {
    label: '产品',
    items: [
      { label: '产品中心', to: '/products', icon: Boxes, permission: 'product.read' },
      { label: 'Product Master 导入', to: '/products/imports', icon: FileUp, permission: 'product.write' },
      { label: 'Feishu 同步', to: '/integrations/feishu', icon: Cable, permission: 'integration.run' },
    ],
  },
  {
    label: '业务',
    items: [
      { label: '访问概览', to: '/analytics', icon: ChartNoAxesCombined, permission: 'analytics.read' },
      { label: '站外来源', to: '/analytics/sources', icon: Waypoints, permission: 'analytics.read' },
      { label: 'RFQ 收件箱', to: '/rfqs', icon: Inbox, permission: 'rfq.read' },
      { label: 'Contact', to: '/contacts', icon: ContactRound, permission: 'rfq.read' },
    ],
  },
  {
    label: '系统',
    items: [
      { label: '用户', to: '/users', icon: UsersRound, permission: 'identity.manage' },
      { label: '角色与权限', to: '/roles', icon: KeyRound, permission: 'identity.manage' },
      { label: '审计日志', to: '/audit', icon: History, permission: 'audit.read' },
      { label: '系统设置', to: '/settings', icon: Settings, permission: 'settings.manage' },
      ...devtoolsNavigation,
    ],
  },
]

export const quickActions: Array<{ label: string; to: string; icon: Component; permission?: Permission }> = [
  { label: '新建内容', to: '/content/new', icon: Newspaper, permission: 'content.write' },
  { label: '导入 Product Master', to: '/products/imports', icon: FileUp, permission: 'product.write' },
  { label: '查看同步差异', to: '/integrations/feishu', icon: SlidersHorizontal, permission: 'integration.run' },
  { label: '处理新 RFQ', to: '/rfqs', icon: FolderKanban, permission: 'rfq.read' },
  { label: '安全设置', to: '/account/security', icon: ShieldCheck },
]
