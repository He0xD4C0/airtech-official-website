<script setup lang="ts">
import { productFamilies, solutions, articles } from '@/content/catalog'
import SectionHeading from '@/components/common/SectionHeading.vue'
import CallToAction from '@/components/common/CallToAction.vue'
import PublishedEditorialSection from '@/components/content/PublishedEditorialSection.vue'
import type { PublicPageModel } from '@/types/content'

defineProps<{ page: PublicPageModel }>()
</script>

<template>
  <main id="main-content">
    <section class="home-hero">
      <div class="shell home-hero-grid">
        <div class="hero-copy">
          <p class="eyebrow light">Industrial airflow engineering</p>
          <h1>Turn airflow requirements into a clear engineering path.</h1>
          <p>Explore fan forms, define operating context and move verified product information into a focused technical inquiry.</p>
          <div class="button-row">
            <a class="button inverted" href="/en/products">Explore products</a>
            <a class="button ghost-light" href="/en/products/selector">Open Fan Selector</a>
          </div>
          <ul class="hero-proof" aria-label="Website principles">
            <li><span aria-hidden="true">✓</span> Explicit units and conditions</li>
            <li><span aria-hidden="true">✓</span> Verified records only</li>
            <li><span aria-hidden="true">✓</span> Contextual RFQ handoff</li>
          </ul>
        </div>
        <div class="airflow-visual" aria-hidden="true">
          <div class="visual-label"><span>INPUT</span><strong>Operating context</strong></div>
          <svg viewBox="0 0 560 440">
            <defs><linearGradient id="air-line" x1="0" x2="1"><stop stop-color="#70d7ae"/><stop offset="1" stop-color="#8dd7ef"/></linearGradient></defs>
            <circle cx="295" cy="216" r="112" fill="none" stroke="rgba(255,255,255,.13)" stroke-width="42" />
            <circle cx="295" cy="216" r="67" fill="rgba(0,0,0,.1)" stroke="rgba(255,255,255,.25)" />
            <g class="fan-blades" fill="url(#air-line)" opacity=".9">
              <path d="M295 207C264 166 252 123 266 85c48 12 76 48 65 94-5 19-17 28-36 28Z"/>
              <path d="M303 215c49-20 94-18 127 6-25 42-67 57-108 34-17-10-23-24-19-40Z"/>
              <path d="M291 224c-18 50-50 81-91 89-17-46-2-88 41-108 18-8 33-2 50 19Z"/>
            </g>
            <circle cx="295" cy="216" r="19" fill="#fff"/>
            <path d="M20 150c70 0 101-41 163-41M13 216h142M25 282c70 0 100 43 163 43" fill="none" stroke="url(#air-line)" stroke-width="4" stroke-linecap="round" opacity=".75"/>
            <path d="M407 128h112M423 216h119M405 304h107" fill="none" stroke="url(#air-line)" stroke-width="4" stroke-linecap="round" opacity=".55"/>
          </svg>
          <div class="visual-metric top"><span>01</span><strong>Discover</strong><small>Fan form & application</small></div>
          <div class="visual-metric bottom"><span>02</span><strong>Qualify</strong><small>Duty point & constraints</small></div>
        </div>
      </div>
    </section>

    <PublishedEditorialSection :content="page.publishedContent" />

    <section class="section shell">
      <SectionHeading eyebrow="Product families" title="Start with fan form—not a flattened technology list" description="EC is modeled as a motor technology that can apply across compatible forms. Exact model records appear only after Product Master validation." />
      <div class="family-strip">
        <a v-for="(family, index) in productFamilies" :key="family.id" :href="`/en/products/${family.slug}`">
          <span>0{{ index + 1 }}</span><strong>{{ family.name }}</strong><small>{{ family.form }}</small>
        </a>
      </div>
    </section>

    <section class="section surface-section">
      <div class="shell">
        <SectionHeading eyebrow="A connected journey" title="Move from a question to a qualified request" />
        <div class="journey-grid">
          <article><span>01</span><h3>Discover</h3><p>Browse by product form, system context or engineering topic.</p><a href="/en/products">View catalog</a></article>
          <article><span>02</span><h3>Define</h3><p>Capture airflow, pressure, environment, electrical and control constraints.</p><a href="/en/products/selector">Use Selector</a></article>
          <article><span>03</span><h3>Evaluate</h3><p>Compare only published model records with explicit missing-data states.</p><a href="/en/products/compare">Open comparison</a></article>
          <article><span>04</span><h3>Hand off</h3><p>Send visible technical context through the correct RFQ journey.</p><a href="/en/request-a-quote">Choose an RFQ</a></article>
        </div>
      </div>
    </section>

    <section class="section shell split-section">
      <div>
        <SectionHeading eyebrow="Application paths" title="Begin with the system around the fan" description="Solution pages organize the questions that influence selection without assuming that every model fits every application." />
        <div class="link-stack">
          <a v-for="solution in solutions.slice(0, 4)" :key="solution.slug" :href="solution.href"><span>{{ solution.title }}</span><span aria-hidden="true">↗</span></a>
        </div>
        <a class="text-link" href="/en/solutions">View all solutions →</a>
      </div>
      <aside class="selector-callout">
        <p class="eyebrow light">Fan Selector</p><h2>Already know the duty point?</h2>
        <p>Enter airflow and pressure with visible units, then add the constraints that can disqualify a candidate.</p>
        <a class="button inverted" href="/en/products/selector">Start with requirements</a>
      </aside>
    </section>

    <section class="section shell">
      <SectionHeading eyebrow="Engineering resources" title="Read the data before making the decision" />
      <div class="card-grid two">
        <article v-for="article in articles" :key="article.slug" class="card editorial-card">
          <p class="eyebrow">{{ article.eyebrow }}</p><h3><a :href="article.href">{{ article.title }}</a></h3><p>{{ article.summary }}</p>
          <ul class="tag-list"><li v-for="tag in article.tags" :key="tag">{{ tag }}</li></ul>
        </article>
      </div>
    </section>
    <CallToAction title="Bring the operating context into the conversation" description="Choose a structured RFQ path for a known product, a selection problem, a project or a replacement." />
  </main>
</template>
