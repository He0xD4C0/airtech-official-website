<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { usePageContext } from 'vike-vue/usePageContext'
import horizontalLogo from '@airtek/ui/assets/brand/airtek-standard-lockup-horizontal.webp'
import BottomActions from '@/app/layout/BottomActions.vue'
import type { PublicPageModel, PublicSiteBootstrap } from '@/shared/types/content'
import { setCurrentAnalyticsContext } from '@/features/analytics/lib/analytics'
import LanguageSwitcher from '@/i18n/LanguageSwitcher.vue'
import { useI18n } from 'vue-i18n'

const pageContext = usePageContext()
const { locale, t } = useI18n({ useScope: 'global' })
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
  <a class="skip-link" href="#main-content" :lang="locale">{{ t('navigation.skipToContent') }}</a>
  <template v-if="site">
    <div v-if="site.brandLine" class="brand-line">{{ site.brandLine }}</div>
    <header class="site-header">
      <div class="shell header-inner">
        <a class="wordmark" :href="site.homePath" :aria-label="t('navigation.home', { brand: site.brandName })">
          <img :src="horizontalLogo" :alt="site.brandName" />
        </a>
        <button
          class="menu-toggle"
          type="button"
          :aria-expanded="menuOpen"
          :aria-label="menuOpen ? t('navigation.closeMenu') : t('navigation.openMenu')"
          aria-controls="primary-navigation"
          @click="menuOpen = !menuOpen"
        >
          <span aria-hidden="true" :lang="locale">{{ menuOpen ? t('navigation.close') : t('navigation.menu') }}</span>
        </button>
        <nav id="primary-navigation" :class="['primary-nav', { open: menuOpen }]" :aria-label="t('navigation.primary')" lang="en">
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
          <LanguageSwitcher />
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
      <nav v-if="site.legalLinks.length" :aria-label="t('navigation.legal')" lang="en">
        <a v-for="link in site.legalLinks" :key="link.href" :href="link.href">{{ link.label }}</a>
      </nav>
    </div>
  </footer>
  <BottomActions />
</template>
