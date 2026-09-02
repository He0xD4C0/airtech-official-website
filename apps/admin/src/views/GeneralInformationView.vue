<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { Eye, Globe2, RotateCcw, Save, Send, ShieldCheck } from 'lucide-vue-next'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { adminApi, type BackendGeneralInformation, type GeneralInformationPayload, type GeneralInformationRevision, type SiteNavigationLink } from '@/services/adminApi'
import { apiErrorMessage, apiProblemStatus } from '@/services/cursorPagination'
import { diffJsonRevisions } from '@/services/jsonRevisionDiff'
import { useAuthStore } from '@/stores/auth'
import { useUiStore } from '@/stores/ui'

const auth = useAuthStore()
const ui = useUiStore()
const state = ref<'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>('loading')
const saving = ref(false)
const entry = ref<BackendGeneralInformation>()
const isPlaceholder = ref(true)
const previewMode = ref<'desktop' | 'mobile'>('desktop')
const revisions = ref<GeneralInformationRevision[]>([])
const selectedRevision = ref<number>()
const autosaveStatus = ref<'idle' | 'pending' | 'saving' | 'saved' | 'error'>('idle')
let autosaveTimer: number | undefined
let hydrating = true
interface GeneralOrganizationForm {
  name: string
  url: string | null
  logoUrl: string | null
  legalName: string | null
  salesEmail: string | null
  marketingEmail: string | null
  address: string | null
  socialLinks: SiteNavigationLink[]
}
interface GeneralInformationForm {
  brandName: string
  brandLine: string | null
  homePath: string
  footerStatement: string | null
  copyrightText: string | null
  defaultSeo: { title: string | null; description: string | null }
  organization: GeneralOrganizationForm
  navigationCta: SiteNavigationLink
}

function recordValue(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null ? value as Record<string, unknown> : {}
}

function nullableString(value: unknown): string | null {
  return typeof value === 'string' ? value : null
}

function navigationLinks(value: unknown): SiteNavigationLink[] {
  if (!Array.isArray(value)) return []
  return value.flatMap((item) => {
    const link = recordValue(item)
    return typeof link.label === 'string' && typeof link.url === 'string'
      ? [{ label: link.label, href: link.url }]
      : []
  })
}

function newForm(): GeneralInformationForm {
  return {
    brandName: '',
    brandLine: null,
    homePath: '',
    footerStatement: null,
    copyrightText: null,
    defaultSeo: { title: null, description: null },
    organization: {
      name: '',
      url: null,
      logoUrl: null,
      legalName: null,
      salesEmail: null,
      marketingEmail: null,
      address: null,
      socialLinks: [],
    },
    navigationCta: { label: '', href: '' },
  }
}

function normalizedForm(payload: GeneralInformationPayload): GeneralInformationForm {
  const defaults = newForm()
  const defaultSeo = recordValue(payload.defaultSeo)
  const organization = recordValue(payload.organization)
  const navigationCta = recordValue(payload.navigationCta)
  return {
    brandName: typeof payload.brandName === 'string' ? payload.brandName : defaults.brandName,
    brandLine: nullableString(payload.brandLine),
    homePath: typeof payload.homePath === 'string' ? payload.homePath : defaults.homePath,
    footerStatement: nullableString(payload.footerStatement),
    copyrightText: nullableString(payload.copyrightText),
    defaultSeo: {
      title: nullableString(defaultSeo.title),
      description: nullableString(defaultSeo.description),
    },
    organization: {
      name: typeof organization.name === 'string' ? organization.name : defaults.organization.name,
      url: nullableString(organization.url),
      logoUrl: nullableString(organization.logoUrl),
      legalName: nullableString(organization.legalName),
      salesEmail: nullableString(organization.salesEmail),
      marketingEmail: nullableString(organization.marketingEmail),
      address: nullableString(organization.address),
      socialLinks: navigationLinks(organization.socialLinks),
    },
    navigationCta: {
      label: typeof navigationCta.label === 'string' ? navigationCta.label : '',
      href: typeof navigationCta.href === 'string' ? navigationCta.href : '',
    },
  }
}

const form = ref<GeneralInformationForm>(newForm())
const selectedSnapshot = computed(() => revisions.value.find((item) => item.currentRevision === selectedRevision.value))
const revisionDiff = computed(() => selectedSnapshot.value ? diffJsonRevisions(selectedSnapshot.value.payload, generalInformationPayload()) : [])

function displayJson(value: unknown): string {
  if (value === undefined) return '—'
  return typeof value === 'string' ? value : JSON.stringify(value)
}

function generalInformationPayload(): GeneralInformationPayload {
  return {
    ...form.value,
    brandName: form.value.brandName.trim(),
    brandLine: form.value.brandLine?.trim() || null,
    homePath: form.value.homePath.trim(),
    footerStatement: form.value.footerStatement?.trim() || null,
    copyrightText: form.value.copyrightText?.trim() || null,
    organization: {
      name: form.value.organization.name.trim(),
      url: form.value.organization.url?.trim() || null,
      logoUrl: form.value.organization.logoUrl?.trim() || null,
      legalName: form.value.organization.legalName?.trim() || null,
      salesEmail: form.value.organization.salesEmail?.trim() || null,
      marketingEmail: form.value.organization.marketingEmail?.trim() || null,
      address: form.value.organization.address?.trim() || null,
      socialLinks: form.value.organization.socialLinks
        .map((item) => ({ label: item.label.trim(), url: item.href.trim() }))
        .filter((item) => item.label && item.url),
    },
    navigationCta: form.value.navigationCta?.label.trim() && form.value.navigationCta.href.trim()
      ? { label: form.value.navigationCta.label.trim(), href: form.value.navigationCta.href.trim() }
      : null,
  }
}

async function loadRevisions(): Promise<void> {
  if (!entry.value) {
    revisions.value = []
    selectedRevision.value = undefined
    return
  }
  revisions.value = await adminApi.listGeneralInformationRevisions(entry.value.id)
  if (!revisions.value.some((item) => item.currentRevision === selectedRevision.value)) {
    selectedRevision.value = revisions.value[0]?.currentRevision
  }
}

function addSocialLink(): void {
  form.value.organization.socialLinks.push({ label: '', href: '' })
}

function removeSocialLink(index: number): void {
  form.value.organization.socialLinks.splice(index, 1)
}

function beginConfiguration(): void {
  if (!auth.hasPermission('content.write')) return
  entry.value = undefined
  form.value = newForm()
  revisions.value = []
  selectedRevision.value = undefined
  state.value = 'ready'
}

async function load(): Promise<void> {
  state.value = 'loading'
  try {
    const result = await adminApi.getGeneralInformation()
    entry.value = result.entry
    form.value = normalizedForm(result.entry.payload)
    isPlaceholder.value = result.entry.isPlaceholder
    await loadRevisions()
    state.value = 'ready'
  } catch (error) {
    const status = apiProblemStatus(error)
    if (status === 404) {
      entry.value = undefined
      state.value = 'empty'
      return
    }
    state.value = apiProblemStatus(error) === 403 ? 'forbidden' : 'error'
    ui.toast('General Information 读取失败', apiErrorMessage(error, '请检查 API。'), 'danger')
  }
}

async function save(silent = false): Promise<boolean> {
  if (state.value !== 'ready' || !auth.hasPermission('content.write')) return false
  saving.value = true
  try {
    autosaveStatus.value = 'saving'
    const payload = generalInformationPayload()
    const result = await adminApi.saveGeneralInformation({ locale: 'en', payload, isPlaceholder: isPlaceholder.value }, entry.value?.id, entry.value?.currentRevision)
    entry.value = result.entry
    autosaveStatus.value = 'saved'
    if (!silent) ui.toast('全站信息已保存', `Working revision ${result.entry.currentRevision}.`)
    return true
  } catch (error) {
    autosaveStatus.value = 'error'
    if (!silent) ui.toast('保存失败', apiErrorMessage(error, '请检查字段和并发版本。'), 'danger')
    return false
  } finally {
    saving.value = false
  }
}

async function publish(): Promise<void> {
  if (!auth.hasPermission('content.publish')) {
    ui.toast('没有发布权限', '当前账号可以编辑草稿，但不能发布全站信息。', 'warning')
    return
  }
  if (!await save(true) || !entry.value) return
  try {
    const result = await adminApi.publishGeneralInformation(entry.value.id, entry.value.currentRevision)
    entry.value = result.entry
    await loadRevisions()
    ui.toast('全站信息已发布', isPlaceholder.value ? '占位信息保持 noindex。' : 'Public site-bootstrap projection 已更新。')
  } catch (error) {
    ui.toast('发布失败', apiErrorMessage(error, '请检查权限和 revision。'), 'danger')
  }
}

async function rollback(): Promise<void> {
  if (!auth.hasPermission('content.publish')) return
  if (!entry.value || selectedRevision.value === undefined) return
  const targetRevision = selectedRevision.value
  const reason = window.prompt(`请输入回滚到 revision ${targetRevision} 的原因：`)?.trim()
  if (!reason) return
  if (reason.length < 10) {
    ui.toast('回滚原因过短', '审计原因至少需要 10 个字符。', 'warning')
    return
  }
  try {
    const result = await adminApi.rollbackGeneralInformation(
      entry.value.id,
      entry.value.currentRevision,
      targetRevision,
      reason,
    )
    hydrating = true
    entry.value = result.entry
    form.value = normalizedForm(result.entry.payload)
    isPlaceholder.value = result.entry.isPlaceholder
    await nextTick()
    hydrating = false
    await loadRevisions()
    ui.toast('已创建回滚 revision', '历史 revision 未被修改，恢复版本已作为新 revision 发布。')
  } catch (error) {
    ui.toast('回滚失败', apiErrorMessage(error, '请检查目标 revision 和权限。'), 'danger')
  }
}

watch([form, isPlaceholder], () => {
  if (hydrating || state.value !== 'ready') return
  autosaveStatus.value = 'pending'
  window.clearTimeout(autosaveTimer)
  autosaveTimer = window.setTimeout(() => void save(true), 1200)
}, { deep: true })

onMounted(async () => {
  await load()
  hydrating = false
})
onBeforeUnmount(() => window.clearTimeout(autosaveTimer))
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="SITE BOOTSTRAP" title="General Information" description="统一管理公开站品牌显示、组织信息、默认 SEO 与全站 CTA；Header/Footer 链接由各自的内容 revision 管理。">
      <template v-if="state === 'ready'" #actions><span class="inline-note">自动保存：{{ { idle: '待编辑', pending: '等待', saving: '保存中', saved: '已保存', error: '失败' }[autosaveStatus] }}</span><button v-if="auth.hasPermission('content.publish')" class="button button--secondary" type="button" :disabled="saving || selectedRevision === undefined" @click="rollback"><RotateCcw :size="16" />回滚所选</button><button v-if="auth.hasPermission('content.write')" class="button button--secondary" type="button" :disabled="saving" @click="save(false)"><Save :size="16" />保存草稿</button><button v-if="auth.hasPermission('content.publish')" class="button button--primary" type="button" :disabled="saving" @click="publish"><Send :size="16" />发布</button></template>
    </PageHeader>

    <DataStatePanel v-if="state !== 'ready'" :state="state" :title="state === 'empty' ? '尚未建立 General Information' : ''" :description="state === 'empty' ? '创建首个英文站点信息 revision 后，公开站才具备完整 site-bootstrap。' : ''" :action-label="state === 'empty' && auth.hasPermission('content.write') ? '开始配置' : ''" @retry="load" @action="beginConfiguration" />

    <template v-else>
      <div class="security-baseline"><ShieldCheck :size="18" /><div><strong>只发布已确认字段</strong><p>空字段会被公开站省略；不会生成公司数字、认证、电话或法律信息。</p></div><StatusBadge :label="entry?.status ?? 'new'" :tone="entry?.status === 'published' ? 'success' : 'neutral'" /></div>
      <div class="general-information-layout">
        <div class="page-stack">
          <section class="panel settings-panel">
            <header><h2>品牌与联系</h2></header>
            <div class="form-grid"><label class="field"><span>品牌显示名</span><input v-model="form.brandName" /></label><label class="field"><span>Brand Line</span><input v-model="form.brandLine" /></label><label class="field"><span>公开站首页路径</span><input v-model="form.homePath" /></label><label class="field"><span>组织显示名称</span><input v-model="form.organization.name" /></label><label class="field"><span>法律名称</span><input v-model="form.organization.legalName" placeholder="未确认时保持为空" /></label><label class="field"><span>组织 URL</span><input v-model="form.organization.url" /></label><label class="field"><span>Logo URL</span><input v-model="form.organization.logoUrl" /></label><label class="field"><span>Sales Email</span><input v-model="form.organization.salesEmail" type="email" /></label><label class="field"><span>Marketing Email</span><input v-model="form.organization.marketingEmail" type="email" /></label><label class="field"><span>Address</span><textarea v-model="form.organization.address" rows="3"></textarea></label><label class="field"><span>Footer statement</span><textarea v-model="form.footerStatement" rows="3"></textarea></label><label class="field"><span>Copyright（支持 {year}）</span><input v-model="form.copyrightText" /></label></div>
          </section>

          <section class="panel settings-panel"><header><h2>默认 SEO</h2></header><div class="form-grid"><label class="field"><span>默认标题</span><input v-model="form.defaultSeo.title" /></label><label class="field"><span>默认描述</span><textarea v-model="form.defaultSeo.description" rows="3"></textarea></label></div></section>

          <section class="panel settings-panel"><header class="panel__header"><h2>Navigation CTA</h2></header><div class="form-grid"><label class="field"><span>Label</span><input v-model="form.navigationCta.label" /></label><label class="field"><span>Href</span><input v-model="form.navigationCta.href" /></label></div><p class="muted-copy">Header 和 Footer 链接由各自的 revisioned content record 管理，此处只保存全站 CTA 引用。</p></section>

          <section class="panel settings-panel"><header class="panel__header"><h2>Social Links</h2><button class="button button--secondary" type="button" @click="addSocialLink">添加链接</button></header><div class="link-editor-list"><div v-for="(link, index) in form.organization.socialLinks" :key="index"><input v-model="link.label" aria-label="社交链接标签" placeholder="Label" /><input v-model="link.href" aria-label="社交链接 URL" placeholder="https://..." /><button class="button button--quiet" type="button" @click="removeSocialLink(index)">移除</button></div><p v-if="!form.organization.socialLinks.length" class="muted-copy">未确认时保持为空。</p></div></section>

          <label class="panel toggle-row general-placeholder"><span><strong>开发占位信息</strong><small>启用时 Public 强制 noindex 且排除 Sitemap/结构化数据。</small></span><input v-model="isPlaceholder" type="checkbox" /></label>
          <section class="panel settings-panel"><header><h2>不可变 Revision 与 Diff</h2></header><label class="field"><span>回滚目标</span><select v-model.number="selectedRevision"><option v-for="item in revisions" :key="item.currentRevision" :value="item.currentRevision">Revision {{ item.currentRevision }} · {{ item.status }}</option></select></label><p v-if="!revisions.length" class="muted-copy">尚无可回滚的发布快照。</p><div v-else class="revision-diff"><strong>与当前 working draft 的 JSON diff（{{ revisionDiff.length }}）</strong><dl><div v-for="difference in revisionDiff.slice(0, 40)" :key="difference.path"><dt><code>{{ difference.path }}</code></dt><dd><del>{{ displayJson(difference.before) }}</del><ins>{{ displayJson(difference.after) }}</ins></dd></div></dl><p v-if="!revisionDiff.length" class="muted-copy">所选 revision 与当前表单一致。</p></div></section>
        </div>

        <aside :class="['panel', 'site-preview', { 'site-preview--mobile': previewMode === 'mobile' }]"><header><Eye :size="18" /><div><p class="eyebrow">LIVE FORM PREVIEW</p><h2>站点壳预览</h2></div><div class="preview-mode-toggle"><button type="button" :class="{ 'is-active': previewMode === 'desktop' }" @click="previewMode = 'desktop'">Desktop</button><button type="button" :class="{ 'is-active': previewMode === 'mobile' }" @click="previewMode = 'mobile'">Mobile</button></div></header><div class="site-preview__bar"><strong>{{ form.brandName || 'Brand not configured' }}</strong><nav><span v-if="form.navigationCta.label">{{ form.navigationCta.label }}</span></nav></div><div class="site-preview__body"><Globe2 :size="32" /><h3>{{ form.brandLine || 'Brand line not configured' }}</h3><p>{{ form.defaultSeo.description || 'Default SEO description is omitted until configured.' }}</p></div><footer><span>{{ form.organization.salesEmail || 'Sales email omitted' }}</span><span>{{ form.organization.address || 'Address omitted' }}</span></footer></aside>
      </div>
    </template>
  </div>
</template>
