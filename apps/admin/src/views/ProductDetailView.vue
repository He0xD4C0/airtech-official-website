<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import {
  ArrowLeft,
  CircleAlert,
  FileUp,
  LockKeyhole,
  RefreshCw,
  Save,
  Send,
  ShieldCheck,
} from 'lucide-vue-next'
import PageHeader from '@/components/PageHeader.vue'
import DataStatePanel from '@/components/DataStatePanel.vue'
import ProductFactsTable from '@/components/ProductFactsTable.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import {
  adminApi,
  type BackendProduct,
  type BackendTemporaryOverride,
  type ProductPrivatePricing,
} from '@/services/adminApi'
import { apiErrorMessage, apiProblemStatus } from '@/services/cursorPagination'
import {
  PRODUCT_FAMILY_LABELS,
  PRODUCT_FAMILY_SLUGS,
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
const savingPresentation = ref(false)
const loadError = ref('')
const loadForbidden = ref(false)
const overrideError = ref('')
const overridesLoaded = ref(false)
const presentationTitle = ref('')
const presentationSummary = ref('')
const presentationSlug = ref('')
const presentationSeoTitle = ref('')
const presentationSeoDescription = ref('')
const presentationIndexable = ref(false)
const presentationSortOrder = ref(0)
const presentationRelatedContentIds = ref('')
const privatePricing = ref<ProductPrivatePricing>()
const pricingState = ref<'idle' | 'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>('idle')

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
  loading.value = true
  loadError.value = ''
  loadForbidden.value = false
  overrideError.value = ''
  overridesLoaded.value = false
  privatePricing.value = undefined
  pricingState.value = 'idle'
  try {
    const found = await adminApi.getProduct(productId.value)
    product.value = found
    presentationTitle.value = found.presentation?.title ?? found.title
    presentationSummary.value = found.presentation?.summary ?? found.summary ?? ''
    presentationSlug.value = found.presentation?.slug ?? found.slug
    presentationSeoTitle.value = found.presentation?.seo.title ?? ''
    presentationSeoDescription.value = found.presentation?.seo.description ?? ''
    presentationIndexable.value = found.presentation?.indexable ?? found.indexable
    presentationSortOrder.value = found.presentation?.sortOrder ?? found.sortOrder
    presentationRelatedContentIds.value = (found.presentation?.relatedContentIds ?? found.relatedContentIds).join('\n')
    try {
      overrides.value = await adminApi.listAllTemporaryOverrides(found.id)
      overridesLoaded.value = true
    } catch (error) {
      overrides.value = []
      overrideError.value = apiErrorMessage(error, '无法读取临时覆盖；发布已停用。')
    }
  } catch (error) {
    product.value = null
    loadForbidden.value = apiProblemStatus(error) === 403
    loadError.value = apiErrorMessage(error, '无法读取产品记录。')
  } finally {
    loading.value = false
  }
}

async function savePresentation(): Promise<void> {
  if (!product.value || savingPresentation.value) return
  savingPresentation.value = true
  try {
    const presentationRevision = product.value.presentation?.revision ?? 0
    const result = await adminApi.updateProductPresentation(product.value.id, presentationRevision, {
      locale: 'en',
      slug: presentationSlug.value.trim(),
      title: presentationTitle.value.trim(),
      summary: presentationSummary.value.trim() || null,
      seo: {
        title: presentationSeoTitle.value.trim() || null,
        description: presentationSeoDescription.value.trim() || null,
        canonicalPath: `/en/products/${PRODUCT_FAMILY_SLUGS[product.value.family]}/${presentationSlug.value.trim()}`,
        indexable: presentationIndexable.value,
      },
      indexable: presentationIndexable.value,
      sortOrder: presentationSortOrder.value,
      relatedContentIds: presentationRelatedContentIds.value.split(/[\s,]+/).map((value) => value.trim()).filter(Boolean),
      reason: 'Admin product presentation update',
    })
    product.value = result.product
    ui.toast('网站展示字段已保存', 'Product Master 来源事实未被修改。')
  } catch (error) {
    ui.toast('网站字段保存失败', apiErrorMessage(error, '请检查 If-Match 和 product.write 权限。'), 'danger')
  } finally {
    savingPresentation.value = false
  }
}

async function loadPrivatePricing(): Promise<void> {
  if (!product.value || !auth.hasPermission('product.pricing.read')) return
  pricingState.value = 'loading'
  privatePricing.value = undefined
  try {
    privatePricing.value = await adminApi.getProductPrivatePricing(product.value.id)
    pricingState.value = Object.keys(privatePricing.value.pricingFields).length ? 'ready' : 'empty'
  } catch (error) {
    pricingState.value = apiProblemStatus(error) === 403 ? 'forbidden' : apiProblemStatus(error) === 404 ? 'empty' : 'error'
  }
}

async function publish(): Promise<void> {
  if (!product.value || !readiness.value.allowed || publishing.value) return
  publishing.value = true
  try {
    const result = await adminApi.publishProduct(product.value.id, product.value.currentRevision)
    ui.toast('产品已发布', `Published revision ${result.product.publishedRevision}.`)
    await load()
  } catch (error) {
    ui.toast('产品发布失败', apiErrorMessage(error, '请检查验证结果、临时覆盖和并发 revision。'), 'danger')
  } finally {
    publishing.value = false
  }
}

onMounted(load)
</script>

<template>
  <div v-if="loading" class="centered-state" role="status">
    <span class="centered-state__icon"><RefreshCw class="spin" :size="28" /></span>
    <h1>正在读取产品记录</h1>
    <p>正在读取专用产品详情资源，并读取全部临时覆盖。</p>
  </div>

  <div v-else-if="!product" class="centered-state">
    <span class="centered-state__icon"><CircleAlert :size="28" /></span>
    <p class="eyebrow">{{ loadForbidden ? 'FORBIDDEN' : 'PRODUCT NOT AVAILABLE' }}</p>
    <h1>{{ loadForbidden ? '没有产品读取权限' : '无法打开产品详情' }}</h1>
    <p>{{ loadError || '产品记录不存在。' }}</p>
    <div class="page-header__actions">
      <RouterLink class="button button--secondary" to="/products"><ArrowLeft :size="16" />返回产品中心</RouterLink>
      <button class="button button--primary" type="button" @click="load"><RefreshCw :size="16" />重试</button>
    </div>
  </div>

  <div v-else class="page-stack product-detail-page">
    <PageHeader eyebrow="PRODUCT MASTER" :title="product.title || product.model || product.stableId" description="来源事实只读；网站展示、SEO 与关系属于 Portal-owned 字段。">
      <template #actions>
        <RouterLink class="button button--secondary" to="/products"><ArrowLeft :size="16" />产品中心</RouterLink>
        <RouterLink v-if="auth.hasPermission('product.write')" class="button button--secondary" to="/products/imports"><FileUp :size="16" />导入历史</RouterLink>
        <button class="button button--quiet" type="button" :disabled="loading" @click="load"><RefreshCw :class="{ spin: loading }" :size="16" />刷新</button>
        <button v-if="auth.hasPermission('product.publish')" class="button button--primary" type="button" :disabled="!readiness.allowed || publishing" :title="readiness.reason" @click="publish">
          <Send :size="16" />{{ publishing ? '正在发布…' : '发布当前 revision' }}
        </button>
      </template>
    </PageHeader>

    <section class="source-authority-banner">
      <div class="source-authority-banner__icon"><ShieldCheck :size="21" /></div>
      <div><strong>{{ product.sourceKind === 'verifiedCsv' ? 'Verified Product Master CSV' : 'Product Master 来源边界' }}</strong><p>{{ readiness.reason }} 来源规格只通过受控导入/同步更新；网站字段不会回写 Product Master 或 Feishu。</p></div>
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
          <div><dt>网站展示 revision</dt><dd>{{ product.presentation?.revision ?? '尚未建立' }}</dd></div>
          <div><dt>已发布展示 revision</dt><dd>{{ product.presentation?.publishedRevision ?? '尚未发布' }}</dd></div>
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

    <section class="panel product-detail-section" aria-labelledby="presentation-heading">
      <header class="panel__header"><div><p class="eyebrow">PORTAL-OWNED</p><h2 id="presentation-heading">网站展示与 SEO</h2></div><button v-if="auth.hasPermission('product.write')" class="button button--primary" type="button" :disabled="savingPresentation" @click="savePresentation"><Save :size="16" />{{ savingPresentation ? '正在保存…' : '保存网站字段' }}</button></header>
      <div class="form-grid"><label class="field"><span>公开标题</span><input v-model="presentationTitle" /></label><label class="field"><span>Slug</span><input v-model="presentationSlug" /></label><label class="field"><span>SEO Title</span><input v-model="presentationSeoTitle" /></label><label class="field"><span>SEO Description</span><textarea v-model="presentationSeoDescription" rows="3"></textarea></label><label class="field"><span>排序值</span><input v-model.number="presentationSortOrder" type="number" /></label></div><label class="field"><span>公开摘要</span><textarea v-model="presentationSummary" rows="4"></textarea></label><label class="field"><span>关联内容 UUID（每行一个）</span><textarea v-model="presentationRelatedContentIds" rows="5" placeholder="留空表示无显式关联"></textarea></label><label class="toggle-row"><span><strong>允许索引</strong><small>仍需已发布且非 placeholder；缺失产品不会生成页面。</small></span><input v-model="presentationIndexable" type="checkbox" /></label>
    </section>

    <section class="panel product-detail-section" aria-labelledby="private-pricing-heading">
      <header class="panel__header"><div><p class="eyebrow">PRIVATE STAGING</p><h2 id="private-pricing-heading">私有报价</h2></div><StatusBadge :label="auth.hasPermission('product.pricing.read') ? '需要按需解密' : '无 pricing.read 权限'" :tone="auth.hasPermission('product.pricing.read') ? 'warning' : 'success'" /></header>
      <div class="security-baseline"><LockKeyhole :size="18" /><div><strong>默认不读取、不显示、不缓存</strong><p>只有点击后才调用私价端点；离开或刷新页面即从内存清除，报价不会进入网站字段。</p></div></div>
      <button v-if="pricingState === 'idle' && auth.hasPermission('product.pricing.read')" class="button button--secondary" type="button" @click="loadPrivatePricing"><LockKeyhole :size="16" />按需查看私有报价</button>
      <DataStatePanel v-else-if="pricingState !== 'idle' && pricingState !== 'ready'" :state="pricingState" :title="pricingState === 'empty' ? '该产品没有可读取的私有报价字段' : ''" @retry="loadPrivatePricing" />
      <div v-else-if="privatePricing" class="data-table-wrap"><table class="data-table"><thead><tr><th>字段</th><th>加密 Staging 解密值</th></tr></thead><tbody><tr v-for="(value, field) in privatePricing.pricingFields" :key="field"><th><code>{{ field }}</code></th><td>{{ value }}</td></tr></tbody></table></div>
    </section>

    <section class="panel product-detail-section" aria-labelledby="assets-heading">
      <header class="panel__header"><div><p class="eyebrow">ASSET RESOLUTION</p><h2 id="assets-heading">附件缺失清单</h2></div><StatusBadge :label="`${product.missingAssets?.length ?? 0} 个未解析`" :tone="product.missingAssets?.length ? 'warning' : 'success'" /></header>
      <div v-if="product.missingAssets?.length" class="data-table-wrap"><table class="data-table"><thead><tr><th>类型</th><th>来源文件名</th><th>状态</th></tr></thead><tbody><tr v-for="asset in product.missingAssets" :key="`${asset.assetType}-${asset.sourceReference}`"><td>{{ asset.assetType }}</td><td><code>{{ asset.sourceReference }}</code></td><td><StatusBadge label="等待上传与人工审核" tone="warning" /></td></tr></tbody></table></div><p v-else class="product-empty-copy">当前产品没有未解析附件引用；实际文件仍须通过媒体人工审核才能公开。</p>
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
      <div v-if="overrideError" class="status-banner"><span>API 错误</span><p>{{ overrideError }}</p></div>
      <div v-else-if="overrides.length" class="data-table-wrap">
        <table class="data-table override-table"><caption class="sr-only">全部临时产品字段覆盖、原因和到期状态</caption><thead><tr><th scope="col">字段路径</th><th scope="col">覆盖值</th><th scope="col">原因</th><th scope="col">创建时间</th><th scope="col">到期时间</th><th scope="col">状态</th></tr></thead><tbody><tr v-for="override in overrides" :key="override.id"><th scope="row"><code>{{ override.fieldPath }}</code></th><td><span class="product-fact-value">{{ formatProductValue(override.value) }}</span></td><td>{{ override.reason }}</td><td><time :datetime="override.createdAt">{{ formatAdminDateTime(override.createdAt) }}</time></td><td><time :datetime="override.expiresAt">{{ formatAdminDateTime(override.expiresAt) }}</time></td><td><StatusBadge :label="isTemporaryOverrideExpired(override) ? '已到期' : '有效'" :tone="isTemporaryOverrideExpired(override) ? 'danger' : 'success'" /></td></tr></tbody></table>
      </div>
      <p v-else class="product-empty-copy">当前产品没有临时覆盖。</p>
    </section>
  </div>
</template>
