<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { usePageContext } from 'vike-vue/usePageContext'
import horizontalLogo from '@airtek/ui/assets/brand/airtek-standard-lockup-horizontal.webp'
import CookieBanner from './CookieBanner.vue'
import CompareTray from '@/components/product/CompareTray.vue'
import type { PublicPageModel, PublicSiteBootstrap } from '@/types/content'
import { setCurrentAnalyticsContext } from '@/lib/analytics'

const pageContext = usePageContext()
const menuOpen = ref(false)
const currentPath = computed(() => pageContext.urlPathname)
const site = computed(() => {
  const data = pageContext.data as { site?: PublicSiteBootstrap } | undefined
  return data?.site
})
const copyright = computed(() => site.value?.copyrightText?.replace('{year}', String(new Date().getFullYear())))

watch(
  () => (pageContext.data as { page?: PublicPageModel } | undefined)?.page?.analyticsContext,
  (context) => setCurrentAnalyticsContext(context),
  { immediate: true },
)

function active(href: string) {
  return currentPath.value === href || (href.startsWith('/en/') && currentPath.value.startsWith(`${href}/`))
}
</script>

<template>
  <a class="skip-link" href="#main-content">Skip to main content</a>
  <template v-if="site">
    <div v-if="site.brandLine" class="brand-line">{{ site.brandLine }}</div>
    <header class="site-header">
      <div class="shell header-inner">
        <a class="wordmark" :href="site.homePath" :aria-label="`${site.brandName} home`">
          <img :src="horizontalLogo" :alt="site.brandName" />
        </a>
        <button
          class="menu-toggle"
          type="button"
          :aria-expanded="menuOpen"
          :aria-label="menuOpen ? 'Close menu' : 'Open menu'"
          aria-controls="primary-navigation"
          @click="menuOpen = !menuOpen"
        >
          <span aria-hidden="true">{{ menuOpen ? 'Close' : 'Menu' }}</span>
        </button>
        <nav id="primary-navigation" :class="['primary-nav', { open: menuOpen }]" aria-label="Primary navigation">
          <a
            v-for="item in site.navigation"
            :key="item.href"
            :href="item.href"
            :aria-current="active(item.href) ? 'page' : undefined"
            @click="menuOpen = false"
          >{{ item.label }}</a>
          <a
            v-if="site.navigationCta"
            class="nav-rfq"
            :href="site.navigationCta.href"
            @click="menuOpen = false"
          >{{ site.navigationCta.label }}</a>
        </nav>
      </div>
    </header>
  </template>

  <slot />

  <footer v-if="site" class="site-footer">
    <div class="shell footer-grid">
      <div>
        <img class="footer-wordmark" :src="horizontalLogo" :alt="site.brandName" />
        <p v-if="site.footerStatement" class="footer-statement">{{ site.footerStatement }}</p>
      </div>
      <div v-for="column in site.footerColumns" :key="column.title">
        <h2>{{ column.title }}</h2>
        <a v-for="link in column.links" :key="link.href" :href="link.href">{{ link.label }}</a>
      </div>
    </div>
    <div v-if="copyright || site.legalLinks.length" class="shell footer-bottom">
      <p v-if="copyright">{{ copyright }}</p>
      <nav v-if="site.legalLinks.length" aria-label="Legal">
        <a v-for="link in site.legalLinks" :key="link.href" :href="link.href">{{ link.label }}</a>
      </nav>
    </div>
  </footer>
  <CompareTray />
  <CookieBanner />
</template>
