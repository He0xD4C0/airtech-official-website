<script setup lang="ts">
import { computed, ref } from 'vue'
import { usePageContext } from 'vike-vue/usePageContext'
import CookieBanner from './CookieBanner.vue'
import CompareTray from '@/components/product/CompareTray.vue'

const pageContext = usePageContext()
const menuOpen = ref(false)
const currentPath = computed(() => pageContext.urlPathname)
const year = new Date().getFullYear()

const nav = [
  { label: 'Products', href: '/en/products' },
  { label: 'Solutions', href: '/en/solutions' },
  { label: 'Technology', href: '/en/technology' },
  { label: 'Resources', href: '/en/resources/articles' },
  { label: 'Company', href: '/en/company/about' },
]

function active(href: string) {
  return currentPath.value === href || currentPath.value.startsWith(`${href}/`)
}
</script>

<template>
  <a class="skip-link" href="#main-content">Skip to main content</a>
  <div class="brand-line">Redefining Airflow with Smart, Green Technology</div>
  <header class="site-header">
    <div class="shell header-inner">
      <a class="wordmark" href="/en" aria-label="AIRTEKPOWER home">AIRTEKPOWER</a>
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
          v-for="item in nav"
          :key="item.href"
          :href="item.href"
          :aria-current="active(item.href) ? 'page' : undefined"
          @click="menuOpen = false"
        >{{ item.label }}</a>
        <a class="nav-rfq" href="/en/request-a-quote" @click="menuOpen = false">Request a quote</a>
      </nav>
    </div>
  </header>

  <slot />

  <footer class="site-footer">
    <div class="shell footer-grid">
      <div>
        <p class="wordmark footer-wordmark">AIRTEKPOWER</p>
        <p class="footer-statement">Clear product data, application context and engineering handoff for industrial airflow decisions.</p>
      </div>
      <div>
        <h2>Explore</h2>
        <a href="/en/products">Products</a>
        <a href="/en/products/selector">Fan Selector</a>
        <a href="/en/solutions">Solutions</a>
        <a href="/en/technology">Technology</a>
      </div>
      <div>
        <h2>Resources</h2>
        <a href="/en/resources/articles">Technical articles</a>
        <a href="/en/resources/faqs">FAQ</a>
        <a href="/en/resources/case-studies">Case studies</a>
        <a href="/en/resources/downloads">Downloads</a>
      </div>
      <div>
        <h2>Company</h2>
        <a href="/en/company/about">About</a>
        <a href="/en/company/contact">Contact</a>
        <a href="/en/request-a-quote">Request a quote</a>
      </div>
    </div>
    <div class="shell footer-bottom">
      <p>© {{ year }} AIRTEKPOWER. All rights reserved.</p>
      <nav aria-label="Legal">
        <a href="/en/privacy">Privacy</a>
        <a href="/en/terms">Terms</a>
        <a href="/en/cookie-settings">Cookie settings</a>
      </nav>
    </div>
  </footer>
  <CompareTray />
  <CookieBanner />
</template>
