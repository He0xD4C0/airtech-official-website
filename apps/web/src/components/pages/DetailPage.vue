<script setup lang="ts">
import PageHero from '@/components/common/PageHero.vue'
import DataNotice from '@/components/common/DataNotice.vue'
import CallToAction from '@/components/common/CallToAction.vue'
import type { PublicPageModel } from '@/types/content'
defineProps<{ page: PublicPageModel }>()
</script>

<template>
  <main id="main-content">
    <PageHero :eyebrow="page.eyebrow" :title="page.title" :description="page.description" :breadcrumbs="page.breadcrumbs" />

    <section v-if="page.collection === 'solutions'" class="section shell article-layout">
      <article class="prose">
        <h2>Begin with system requirements</h2>
        <p>A useful application review starts with the required airflow and pressure, available installation space, ambient conditions, electrical supply, control strategy and any model-scoped compliance needs.</p>
        <h2>Separate discovery from suitability</h2>
        <p>This solution path helps organize relevant products and technical content. It does not assert that every catalog family or model is suitable for this application.</p>
        <h2>Prepare an engineering handoff</h2>
        <ul><li>State the duty point with units and operating conditions.</li><li>Identify environmental and dimensional hard constraints.</li><li>Describe electrical, control and feedback requirements.</li><li>Record any required standard, market or project evidence.</li></ul>
      </article>
      <aside class="side-panel"><p class="eyebrow">Related actions</p><h2>Continue the journey</h2><a href="/en/products/selector">Use Fan Selector</a><a href="/en/technology/airflow-and-pressure">Review airflow & pressure</a><a href="/en/request-a-quote/project">Start Project RFQ</a></aside>
    </section>

    <section v-else-if="page.collection === 'technology'" class="section shell article-layout">
      <article class="prose">
        <h2>What this topic clarifies</h2>
        <p>Technical information becomes useful when its definition, unit, operating condition, source and applicability are explicit. This page is structured to connect those details to published product records.</p>
        <h2>Data required for a decision</h2>
        <p>Exact values stay attached to a model and revision. Generic technical explanations never substitute for an approved product curve, manual, certificate or test record.</p>
        <blockquote>When a required condition is unknown, the interface reports the uncertainty and routes the question for engineering review.</blockquote>
        <h2>Related product fields</h2>
        <ul><li>Stable identity and classification</li><li>Electrical and control conditions</li><li>Declared operating points and performance curves</li><li>Measurement and test context</li></ul>
      </article>
      <aside class="side-panel"><p class="eyebrow">Explore</p><a href="/en/products">Product families</a><a href="/en/resources/articles">Technical articles</a><a href="/en/resources/faqs/technical">Technical FAQ</a></aside>
    </section>

    <section v-else-if="page.collection === 'articles'" class="section shell article-layout">
      <article v-if="page.slug === 'reading-a-fan-curve'" class="prose">
        <p class="article-lead">A fan curve relates airflow to pressure for a defined configuration. Its axes alone are not enough to establish suitability.</p>
        <h2>Read the axes before the line</h2><p>Airflow is volume per time. Pressure must be identified as static, total or another declared measure. Keep source units visible and document any conversion.</p>
        <h2>Find the operating conditions</h2><p>Speed, air density, voltage, test setup and method affect interpretation. A curve without its declared context is incomplete evidence.</p>
        <h2>Use the system requirement</h2><p>The required duty point and system resistance help frame candidate evaluation. Environmental, dimensional, electrical and compliance requirements remain hard constraints.</p>
        <h2>Escalate uncertainty</h2><p>When curves or conditions are incomplete, selection should report uncertainty and request engineering review instead of producing a confident-looking score.</p>
      </article>
      <article v-else class="prose">
        <p class="article-lead">Blank, zero, not applicable and pending verification describe different realities. Product systems should preserve that difference.</p>
        <h2>Why zero is dangerous</h2><p>Zero is a measurement. Using it for a missing value can silently affect filters, comparisons and selector rules.</p>
        <h2>Use explicit states</h2><p>A controlled record distinguishes verified, missing, not applicable, not tested, confidential and pending verification values.</p>
        <h2>Carry the state to the interface</h2><p>Filters exclude records that cannot satisfy a hard constraint; comparison tables show honest gaps; product pages explain when engineering review is required.</p>
      </article>
      <aside class="side-panel"><p class="eyebrow">Article principles</p><p>No model-specific conclusion is made without validated product data.</p><a href="/en/products/selector">Open Fan Selector</a><a href="/en/resources/faqs">Browse FAQ</a></aside>
    </section>

    <section v-else class="section shell">
      <DataNotice title="Case publication held" text="This template remains noindex until the problem, configuration, operating context, results, evidence and publication permission are approved." />
      <div class="case-placeholder"><div><span>01</span><h2>Problem and baseline</h2><p>Awaiting an approved, appropriately anonymized case record.</p></div><div><span>02</span><h2>Configuration</h2><p>Awaiting exact product, operating point and engineering intervention.</p></div><div><span>03</span><h2>Measured result</h2><p>Awaiting units, method, duration, comparison basis and evidence owner.</p></div></div>
    </section>
    <CallToAction title="Apply the same evidence discipline to your project" description="Share the system context and constraints for a focused engineering conversation." href="/en/request-a-quote/project" label="Start Project RFQ" />
  </main>
</template>
