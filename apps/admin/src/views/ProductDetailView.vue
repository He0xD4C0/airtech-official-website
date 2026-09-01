<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import {
  ArrowLeft,
  CircleAlert,
  Database,
  RefreshCw,
  Send,
  ShieldCheck,
} from 'lucide-vue-next'
import PageHeader from '@/components/PageHeader.vue'
import ProductFactsTable from '@/components/ProductFactsTable.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import {
  adminApi,
  mockApiEnabled,
  type BackendProduct,
  type BackendTemporaryOverride,
} from '@/services/adminApi'
import { apiErrorMessage } from '@/services/cursorPagination'
import {
  PRODUCT_FAMILY_LABELS,
  factStatePresentation,
  formatAdminDateTime,
  formatProductValue,
  isTemporaryOverrideExpired,
  productPublishReadiness,
  publicationStatusPresentation,
  verifiedPerformanceCurves,
} from '@/services/productPresentation'
import { useAuthStore } from '@/stores/auth'
import { useUiStore } from '@/stores/ui'

const route = useRoute()
const auth = useAuthStore()
const ui = useUiStore()
const product = ref<BackendProduct | null>(null)
const overrides = ref<BackendTemporaryOverride[]>([])
const loading = ref(false)
const publishing = ref(false)
const loadError = ref('')
const overrideError = ref('')
const overridesLoaded = ref(false)

const productId = computed(() => String(route.params.id ?? ''))
const verifiedCurves = computed(() => product.value ? verifiedPerformanceCurves(product.value) : [])
const excludedCurveCount = computed(() => (product.value?.performanceCurves.length ?? 0) - verifiedCurves.value.length)
const conditionedSpecifications = computed(() => product.value?.specifications.filter((specification) => specification.operatingCondition) ?? [])
const readiness = computed(() => {
  if (!product.value) return { allowed: false, reason: '产品记录尚未加载。' }
  if (!overridesLoaded.value) return { allowed: false, reason: '临时覆盖未完整加载，不能安全发布。' }
  return productPublishReadiness(product.value, overrides.value, auth.hasPermission('product.publish'))
})

async function load(): Promise<void> {
  if (mockApiEnabled) return
  loading.value = true
  loadError.value = ''
  overrideError.value = ''
  overridesLoaded.value = false
  try {
    const found = await adminApi.findProduct(productId.value)
    if (!found) {
      product.value = null
      loadError.value = '未在 Product Master working records 中找到该产品。'
      return
    }
    product.value = found
    try {
      overrides.value = await adminApi.listAllTemporaryOverrides(found.id)
      overridesLoaded.value = true
    } catch (error) {
      overrides.value = []
      overrideError.value = apiErrorMessage(error, '无法读取临时覆盖；发布已停用。')
    }
  } catch (error) {
    product.value = null
    loadError.value = apiErrorMessage(error, '无法读取产品记录。')
  } finally {
    loading.value = false
  }
}

async function publish(): Promise<void> {
  if (!product.value || !readiness.value.allowed || publishing.value) return
  publishing.value = true
  try {
    const result = await adminApi.publishProduct(product.value.id, product.value.currentRevision)
    product.value = result.product
    ui.toast('产品已发布', `Published revision ${result.product.publishedRevision}.`)
  } catch (error) {
    ui.toast('产品发布失败', apiErrorMessage(error, '请检查验证结果、临时覆盖和并发 revision。'), 'danger')
  } finally {
    publishing.value = false
  }
}

onMounted(load)
</script>

<template>
  <div v-if="mockApiEnabled" class="centered-state">
    <span class="centered-state__icon"><Database :size="28" /></span>
    <p class="eyebrow">PRODUCT MASTER</p>
    <h1>开发演示没有真实产品详情</h1>
    <p>此页面只展示 Rust Admin API 返回的 Product Master working record；不会用演示型号、规格或 PQ 曲线填充。</p>
    <RouterLink class="button button--secondary" to="/products"><ArrowLeft :size="16" />返回产品中心</RouterLink>
  </div>

  <div v-else-if="loading" class="centered-state" role="status">
    <span class="centered-state__icon"><RefreshCw class="spin" :size="28" /></span>
    <h1>正在读取产品记录</h1>
    <p>会跨 cursor 查找稳定产品记录，并读取全部临时覆盖。</p>
  </div>

  <div v-else-if="!product" class="centered-state">
    <span class="centered-state__icon"><CircleAlert :size="28" /></span>
    <p class="eyebrow">PRODUCT NOT AVAILABLE</p>
    <h1>无法打开产品详情</h1>
    <p>{{ loadError || '产品记录不存在。' }}</p>
    <div class="page-header__actions">
      <RouterLink class="button button--secondary" to="/products"><ArrowLeft :size="16" />返回产品中心</RouterLink>
      <button class="button button--primary" type="button" @click="load"><RefreshCw :size="16" />重试</button>
    </div>
  </div>

  <div v-else class="page-stack product-detail-page">
    <PageHeader eyebrow="PRODUCT MASTER" :title="product.title || product.model || product.stableId" description="只读工作记录；Feishu-owned 产品事实必须通过同步与冲突流程变更。">
      <template #actions>
        <RouterLink class="button button--secondary" to="/products"><ArrowLeft :size="16" />产品中心</RouterLink>
        <button class="button button--quiet" type="button" :disabled="loading" @click="load"><RefreshCw :class="{ spin: loading }" :size="16" />刷新</button>
        <button class="button button--primary" type="button" :disabled="!readiness.allowed || publishing" :title="readiness.reason" @click="publish">
          <Send :size="16" />{{ publishing ? '正在发布…' : '发布当前 revision' }}
        </button>
      </template>
    </PageHeader>

    <section class="source-authority-banner">
      <div class="source-authority-banner__icon"><ShieldCheck :size="21" /></div>
      <div><strong>Product Master 只读边界</strong><p>{{ readiness.reason }} 本页不会直接编辑来源字段，也不会将临时覆盖回写 Feishu。</p></div>
      <StatusBadge v-bind="publicationStatusPresentation(product.status)" />
    </section>

    <div class="product-detail-grid">
      <section class="panel product-card-preview" aria-labelledby="public-card-heading">
        <header><div><p class="eyebrow">PUBLIC CARD PREVIEW</p><h2 id="public-card-heading">公开卡片预览</h2></div><StatusBadge :label="product.indexable ? '可索引记录' : 'noindex'" :tone="product.indexable ? 'success' : 'warning'" /></header>
        <div class="product-card-preview__visual" aria-hidden="true"><span>AIRTEKPOWER</span></div>
        <div class="product-card-preview__body">
          <p>{{ PRODUCT_FAMILY_LABELS[product.family] }}<template v-if="product.subtype"> · {{ product.subtype }}</template></p>
          <h3>{{ product.title || product.model || product.stableId }}</h3>
          <strong v-if="product.model">{{ product.model }}</strong>
          <span>{{ product.summary || '当前 working record 未提供公开摘要。' }}</span>
          <code>{{ product.stableId }}</code>
        </div>
        <footer>此预览直接使用当前 API 字段，不补齐图片、性能或认证声明。</footer>
      </section>

      <section class="panel product-identity-panel" aria-labelledby="identity-heading">
        <header class="panel__header"><div><p class="eyebrow">IDENTITY & SOURCE</p><h2 id="identity-heading">标识、来源与 revision</h2></div></header>
        <dl class="product-detail-list">
          <div><dt>稳定 ID</dt><dd><code>{{ product.stableId }}</code></dd></div>
          <div><dt>记录 UUID</dt><dd><code>{{ product.id }}</code></dd></div>
          <div><dt>来源快照</dt><dd><code>{{ product.sourceSnapshotId }}</code></dd></div>
          <div><dt>来源 revision</dt><dd><code>{{ product.sourceRevision }}</code></dd></div>
          <div><dt>当前 revision</dt><dd>{{ product.currentRevision }}</dd></div>
          <div><dt>已发布 revision</dt><dd>{{ product.publishedRevision ?? '尚未发布' }}</dd></div>
          <div><dt>Locale</dt><dd>{{ product.locale }}</dd></div>
          <div><dt>Slug</dt><dd><code>{{ product.slug }}</code></dd></div>
          <div><dt>更新时间</dt><dd><time :datetime="product.updatedAt">{{ formatAdminDateTime(product.updatedAt) }}</time></dd></div>
        </dl>
      </section>
    </div>

    <section class="panel product-detail-section" aria-labelledby="classification-heading">
      <header class="panel__header"><div><p class="eyebrow">TAXONOMY</p><h2 id="classification-heading">分类</h2></div></header>
      <dl class="product-classification-list">
        <div><dt>产品家族</dt><dd>{{ PRODUCT_FAMILY_LABELS[product.family] }}</dd></div>
        <div><dt>Subtype</dt><dd>{{ product.subtype || '未提供' }}</dd></div>
        <div><dt>Motor technology facet</dt><dd>{{ product.motorTechnology || '未提供' }}</dd></div>
        <div><dt>型号</dt><dd>{{ product.model || '未提供' }}</dd></div>
      </dl>
    </section>

    <section class="panel product-detail-section" aria-labelledby="specifications-heading">
      <header class="panel__header"><div><p class="eyebrow">PRODUCT FACTS</p><h2 id="specifications-heading">规格与 Fact state</h2></div><span class="inline-note">{{ product.specifications.length }} 条来源事实</span></header>
      <ProductFactsTable :specifications="product.specifications" />
    </section>

    <section class="panel product-detail-section" aria-labelledby="conditions-heading">
      <header class="panel__header"><div><p class="eyebrow">OPERATING CONDITIONS</p><h2 id="conditions-heading">工况引用</h2></div></header>
      <div v-if="conditionedSpecifications.length" class="data-table-wrap">
        <table class="data-table"><caption class="sr-only">明确附带工况的规格事实</caption><thead><tr><th scope="col">规格</th><th scope="col">工况</th><th scope="col">来源</th></tr></thead><tbody><tr v-for="specification in conditionedSpecifications" :key="specification.key"><th scope="row">{{ specification.label }}</th><td>{{ specification.operatingCondition }}</td><td>{{ specification.sourceReference || '—' }}</td></tr></tbody></table>
      </div>
      <p v-else class="product-empty-copy">当前规格没有显式工况引用。曲线工况如有，将在各 PQ 数据表上单独显示。</p>
    </section>

    <section class="panel product-detail-section" aria-labelledby="curves-heading">
      <header class="panel__header"><div><p class="eyebrow">PERFORMANCE DATA</p><h2 id="curves-heading">已验证 PQ 曲线</h2></div><StatusBadge :label="`${verifiedCurves.length} 条 verified`" :tone="verifiedCurves.length ? 'success' : 'neutral'" /></header>
      <p v-if="excludedCurveCount" class="product-section-note">另有 {{ excludedCurveCount }} 条非 verified 曲线未在性能数据表中公开展示。</p>
      <article v-for="(curve, curveIndex) in verifiedCurves" :key="`${curve.sourceReference}-${curveIndex}`" class="pq-record">
        <header><h3>曲线 {{ curveIndex + 1 }}</h3><StatusBadge v-bind="factStatePresentation(curve.state)" /></header>
        <dl class="pq-record__conditions">
          <div><dt>转速</dt><dd>{{ curve.speedRpm === null ? '未提供' : `${curve.speedRpm} rpm` }}</dd></div>
          <div><dt>空气密度</dt><dd>{{ curve.densityKgM3 === null ? '未提供' : `${curve.densityKgM3} kg/m³` }}</dd></div>
          <div><dt>电压</dt><dd>{{ curve.voltage || '未提供' }}</dd></div>
          <div><dt>测试方法</dt><dd>{{ curve.testMethod || '未提供' }}</dd></div>
          <div><dt>来源</dt><dd>{{ curve.sourceReference }}</dd></div>
        </dl>
        <div v-if="curve.points.length" class="data-table-wrap">
          <table class="data-table pq-table"><caption>曲线 {{ curveIndex + 1 }}：Airflow（{{ curve.airflowUnit }}）与 pressure（{{ curve.pressureUnit }}）</caption><thead><tr><th scope="col">点</th><th scope="col">Airflow ({{ curve.airflowUnit }})</th><th scope="col">Pressure ({{ curve.pressureUnit }})</th></tr></thead><tbody><tr v-for="(point, pointIndex) in curve.points" :key="pointIndex"><th scope="row">{{ pointIndex + 1 }}</th><td>{{ point.airflow }}</td><td>{{ point.pressure }}</td></tr></tbody></table>
        </div>
        <p v-else class="product-empty-copy">API 将此曲线标记为 verified，但未返回性能点。</p>
      </article>
      <p v-if="!verifiedCurves.length" class="product-empty-copy">当前记录没有状态为 verified 的 PQ 曲线。</p>
    </section>

    <section class="panel product-detail-section" aria-labelledby="overrides-heading">
      <header class="panel__header"><div><p class="eyebrow">TEMPORARY OVERRIDES</p><h2 id="overrides-heading">临时覆盖与到期状态</h2></div><span class="inline-note">只读；不回写 Feishu</span></header>
      <div v-if="overrideError" class="demo-banner"><span>API 错误</span><p>{{ overrideError }}</p></div>
      <div v-else-if="overrides.length" class="data-table-wrap">
        <table class="data-table override-table"><caption class="sr-only">全部临时产品字段覆盖、原因和到期状态</caption><thead><tr><th scope="col">字段路径</th><th scope="col">覆盖值</th><th scope="col">原因</th><th scope="col">创建时间</th><th scope="col">到期时间</th><th scope="col">状态</th></tr></thead><tbody><tr v-for="override in overrides" :key="override.id"><th scope="row"><code>{{ override.fieldPath }}</code></th><td><span class="product-fact-value">{{ formatProductValue(override.value) }}</span></td><td>{{ override.reason }}</td><td><time :datetime="override.createdAt">{{ formatAdminDateTime(override.createdAt) }}</time></td><td><time :datetime="override.expiresAt">{{ formatAdminDateTime(override.expiresAt) }}</time></td><td><StatusBadge :label="isTemporaryOverrideExpired(override) ? '已到期' : '有效'" :tone="isTemporaryOverrideExpired(override) ? 'danger' : 'success'" /></td></tr></tbody></table>
      </div>
      <p v-else class="product-empty-copy">当前产品没有临时覆盖。</p>
    </section>
  </div>
</template>
