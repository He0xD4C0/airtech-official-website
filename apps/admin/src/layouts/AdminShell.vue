<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { RouterLink, RouterView, useRoute, useRouter } from 'vue-router'
import {
  Command,
  LogOut,
  Menu,
  PanelLeftClose,
  Search,
  ShieldCheck,
  X,
} from 'lucide-vue-next'
import BrandMark from '@/components/BrandMark.vue'
import { navigation, quickActions } from '@/config/navigation'
import { adminApi } from '@/services/adminApi'
import { contentApi } from '@/services/contentApi'
import { routeComponentKey } from '@/router/routeComponentKey'
import { useAuthStore } from '@/stores/auth'
import { useUiStore } from '@/stores/ui'

const auth = useAuthStore()
const ui = useUiStore()
const route = useRoute()
const router = useRouter()
const navigationBadges = ref<Record<string, string>>({})
const quickQuery = ref('')
const commandInput = ref<HTMLInputElement>()
const commandTrigger = ref<HTMLButtonElement>()

const visibleNavigation = computed(() => navigation
  .map((group) => ({
    ...group,
    items: group.items.filter((item) => (
      (!item.devOnly || __AIRTEK_DEVTOOLS__)
      && auth.hasPermission(item.permission)
      && (!item.permissionsAny?.length || item.permissionsAny.some((permission) => auth.hasPermission(permission)))
    )),
  }))
  .filter((group) => group.items.length > 0))

const pageTitle = computed(() => String(route.meta.title ?? '管理平台'))
const userInitials = computed(() => auth.user?.displayName.slice(0, 2).toUpperCase() ?? 'AT')
const visibleQuickActions = computed(() => {
  const query = quickQuery.value.trim().toLocaleLowerCase()
  return quickActions.filter((action) => (
    auth.hasPermission(action.permission)
    && (!query || action.label.toLocaleLowerCase().includes(query))
  ))
})

async function loadNavigationBadges(): Promise<void> {
  const badges: Record<string, string> = {}
  const requests: Array<Promise<void>> = []

  if (auth.hasPermission('rfq.read')) {
    requests.push(adminApi.listRfqs({ limit: 1 }).then((rfqs) => {
      if (rfqs.total) badges['/rfqs'] = String(rfqs.total)
    }))
  }
  if (auth.hasPermission('content.publish')) {
    requests.push(contentApi.listReviews({ limit: 1 }).then((page) => {
      if (page.total > 0) badges['/content/reviews'] = `${page.total}`
    }))
  }

  await Promise.allSettled(requests)
  navigationBadges.value = badges
}

async function signOut(): Promise<void> {
  await auth.logout()
  await router.push({ name: 'login' })
}

function openCommand(): void {
  ui.commandOpen = true
}

function closeCommand(): void {
  ui.commandOpen = false
}

function handleKeydown(event: KeyboardEvent): void {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
    event.preventDefault()
    ui.commandOpen = !ui.commandOpen
  }
  if (event.key === 'Escape') {
    ui.commandOpen = false
    ui.closeSidebar()
  }
}

onMounted(() => {
  window.addEventListener('keydown', handleKeydown)
  void loadNavigationBadges()
})
onBeforeUnmount(() => window.removeEventListener('keydown', handleKeydown))

watch(() => ui.commandOpen, async (open) => {
  if (open) {
    quickQuery.value = ''
    await nextTick()
    commandInput.value?.focus()
  } else {
    commandTrigger.value?.focus()
  }
})
</script>

<template>
  <div class="admin-shell">
    <a href="#main-content" class="skip-link">跳到主要内容</a>
    <div v-if="ui.sidebarOpen" class="shell-scrim" @click="ui.closeSidebar"></div>

    <aside class="sidebar" :class="{ 'sidebar--open': ui.sidebarOpen }">
      <div class="sidebar__brand">
        <BrandMark compact />
        <button class="icon-button sidebar__mobile-close" type="button" aria-label="关闭导航" @click="ui.closeSidebar">
          <X :size="18" />
        </button>
      </div>

      <div class="sidebar__context">
        <span class="sidebar__context-label">管理空间</span>
        <div class="sidebar__context-static">
          <span class="sidebar__context-mark">AP</span>
          <span><strong>AIRTEKPOWER</strong><small>Global · English</small></span>
        </div>
      </div>

      <nav class="sidebar__nav" aria-label="管理后台主导航">
        <section v-for="group in visibleNavigation" :key="group.label">
          <p>{{ group.label }}</p>
          <RouterLink
            v-for="item in group.items"
            :key="item.to"
            :to="item.to"
            :class="{ 'is-active': route.path === item.to || (item.to !== '/' && route.path.startsWith(`${item.to}/`)) }"
            @click="ui.closeSidebar"
          >
            <component :is="item.icon" :size="18" aria-hidden="true" />
            <span>{{ item.label }}</span>
            <em v-if="navigationBadges[item.to]">{{ navigationBadges[item.to] }}</em>
          </RouterLink>
        </section>
      </nav>

      <div class="sidebar__footer">
        <RouterLink class="sidebar__security" to="/account/security"><ShieldCheck :size="16" /><span>安全会话</span><i></i></RouterLink>
        <button type="button" class="sidebar__user" @click="signOut">
          <span class="avatar">{{ userInitials }}</span>
          <span><strong>{{ auth.user?.displayName }}</strong><small>{{ auth.user?.role }}</small></span>
          <LogOut :size="16" aria-label="退出登录" />
        </button>
      </div>
    </aside>

    <section class="workspace">
      <header class="topbar">
        <div class="topbar__left">
          <button class="icon-button topbar__menu" type="button" aria-label="打开导航" @click="ui.toggleSidebar">
            <Menu :size="20" />
          </button>
          <PanelLeftClose :size="18" class="topbar__context-icon" aria-hidden="true" />
          <div><small>管理平台</small><strong>{{ pageTitle }}</strong></div>
        </div>
        <div class="topbar__actions">
          <button ref="commandTrigger" type="button" class="command-trigger" @click="openCommand">
            <Search :size="17" /><span>快速导航</span><kbd><Command :size="11" /> K</kbd>
          </button>
          <span v-if="auth.isDevelopment" class="environment-chip">开发环境</span>
        </div>
      </header>

      <main id="main-content" class="workspace__main" tabindex="-1">
        <RouterView v-slot="{ Component, route: activeRoute }">
          <component :is="Component" :key="routeComponentKey(activeRoute)" />
        </RouterView>
      </main>
    </section>

    <div v-if="ui.commandOpen" class="command-overlay" role="presentation" @click.self="closeCommand">
      <section class="command-panel" role="dialog" aria-modal="true" aria-label="快速导航">
        <header><Search :size="20" /><input ref="commandInput" v-model="quickQuery" aria-label="筛选快速导航" placeholder="筛选可用操作…" /><kbd>Esc</kbd></header>
        <p>快速导航</p>
        <RouterLink v-for="action in visibleQuickActions" :key="action.to" :to="action.to" @click="closeCommand">
          <span><component :is="action.icon" :size="18" /></span>{{ action.label }}<small>打开</small>
        </RouterLink>
        <p v-if="!visibleQuickActions.length" class="empty-mini">没有匹配的可用操作。</p>
      </section>
    </div>
  </div>
</template>
