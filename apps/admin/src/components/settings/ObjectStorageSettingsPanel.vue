<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { CheckCircle2, Database, FlaskConical, Save } from 'lucide-vue-next'
import {
  adminApi,
  type ObjectStorageSettings,
  type ObjectStorageSettingsInput,
  type UpdateObjectStorageSettings,
} from '@/services/adminApi'
import { apiErrorMessage } from '@/services/cursorPagination'
import { useUiStore } from '@/stores/ui'
import type { ApiProblem } from '@/types/domain'

const ui = useUiStore()
const loaded = ref<ObjectStorageSettings | null>(null)
const endpoint = ref('')
const region = ref('us-east-1')
const bucket = ref('')
const accessKeyId = ref('')
const secretAccessKey = ref('')
const keyPrefix = ref('media')
const pathStyle = ref(false)
const publicBaseUrl = ref('')
const reason = ref('')
const adoptLegacyAssets = ref(false)
const loading = ref(true)
const testing = ref(false)
const saving = ref(false)
const error = ref('')
const testedUrl = ref('')

const configuredLabel = computed(() => loaded.value?.configured ? '已配置' : '未配置')
const needsAdoption = computed(() => (loaded.value?.legacyAssetCount ?? 0) > 0)

function apply(settings: ObjectStorageSettings): void {
  loaded.value = settings
  endpoint.value = settings.endpoint ?? ''
  region.value = settings.region ?? 'us-east-1'
  bucket.value = settings.bucket ?? ''
  accessKeyId.value = settings.accessKeyId ?? ''
  secretAccessKey.value = ''
  keyPrefix.value = settings.keyPrefix ?? 'media'
  pathStyle.value = settings.pathStyle
  publicBaseUrl.value = settings.publicBaseUrl ?? ''
  adoptLegacyAssets.value = settings.legacyAssetCount === 0
  testedUrl.value = ''
}

function payload(): ObjectStorageSettingsInput | null {
  const values = {
    endpoint: endpoint.value.trim().replace(/\/$/, ''),
    region: region.value.trim(),
    bucket: bucket.value.trim(),
    accessKeyId: accessKeyId.value.trim(),
    secretAccessKey: secretAccessKey.value,
    keyPrefix: keyPrefix.value.trim().replace(/^\/+|\/+$/g, ''),
    pathStyle: pathStyle.value,
    publicBaseUrl: publicBaseUrl.value.trim().replace(/\/$/, ''),
  }
  if (!values.endpoint || !values.region || !values.bucket || !values.accessKeyId
    || !values.keyPrefix || !values.publicBaseUrl
    || (!loaded.value?.secretConfigured && !values.secretAccessKey)) {
    ui.toast('对象存储字段不完整', '请填写所有连接字段；首次配置必须提供 Secret Access Key。', 'warning')
    return null
  }
  return values
}

async function load(): Promise<void> {
  loading.value = true
  error.value = ''
  try {
    const result = await adminApi.getObjectStorageSettings()
    apply(result.settings)
  } catch (caught) {
    error.value = apiErrorMessage(caught, '无法读取对象存储设置。')
    ui.toast('对象存储设置读取失败', error.value, 'danger')
  } finally {
    loading.value = false
  }
}

async function testConnection(): Promise<void> {
  const input = payload()
  if (!input) return
  testing.value = true
  testedUrl.value = ''
  try {
    const result = await adminApi.testObjectStorageSettings(input)
    testedUrl.value = result.publicUrl
    ui.toast('对象存储测试通过', '写入、匿名公开读取和清理均已完成。', 'success')
  } catch (caught) {
    ui.toast('对象存储测试失败', problemMessage(caught), 'danger')
  } finally {
    testing.value = false
  }
}

async function save(): Promise<void> {
  const input = payload()
  const baseline = loaded.value
  if (!input || !baseline) return
  if ([...reason.value.trim()].length < 12) {
    ui.toast('需要变更原因', '原因至少填写 12 个字符，并会进入脱敏审计记录。', 'warning')
    return
  }
  if (needsAdoption.value && !adoptLegacyAssets.value) {
    ui.toast('需要接管历史媒体', '确认历史媒体的公开基础地址后才能保存。', 'warning')
    return
  }
  const update: UpdateObjectStorageSettings = {
    ...input,
    adoptLegacyAssets: adoptLegacyAssets.value,
    reason: reason.value.trim(),
  }
  saving.value = true
  try {
    const result = await adminApi.updateObjectStorageSettings(update, baseline.revision)
    apply(result.settings)
    reason.value = ''
    ui.toast('对象存储设置已保存', `已保存 revision ${result.settings.revision}；Secret 不会由 API 回显。`, 'success')
  } catch (caught) {
    const problem = caught as ApiProblem
    ui.toast(problem.status === 409 ? '设置发生并发冲突' : '对象存储保存失败', problemMessage(caught), 'danger')
    if (problem.status === 409) await load()
  } finally {
    saving.value = false
  }
}

function problemMessage(error: unknown): string {
  return apiErrorMessage(error, '对象存储请求失败。')
}

onMounted(() => void load())
</script>

<template>
  <div class="object-storage-settings">
    <div v-if="loading" class="inline-note">正在读取数据库中的对象存储设置…</div>
    <div v-else-if="error" class="storage-error" role="alert">
      <p>{{ error }}</p><button class="button button--secondary" type="button" @click="load">重试</button>
    </div>
    <template v-else-if="loaded">
      <div class="storage-status">
        <Database :size="18" />
        <span><strong>{{ configuredLabel }}</strong><small>revision {{ loaded.revision }} · Secret {{ loaded.secretConfigured ? '已保存且不回显' : '未保存' }}</small></span>
      </div>

      <div class="settings-section">
        <h3>S3 兼容连接</h3>
        <p>这些值只写入 PostgreSQL，不再从 `.env` 或容器环境变量读取。</p>
        <div class="form-grid">
          <label class="field"><span>API Endpoint</span><input v-model="endpoint" placeholder="http://minio:9000" autocomplete="off" /></label>
          <label class="field"><span>Region</span><input v-model="region" placeholder="us-east-1" autocomplete="off" /></label>
          <label class="field"><span>Bucket</span><input v-model="bucket" placeholder="airtek-media" autocomplete="off" /></label>
          <label class="field"><span>Key Prefix</span><input v-model="keyPrefix" placeholder="media" autocomplete="off" /></label>
          <label class="field"><span>Access Key ID</span><input v-model="accessKeyId" autocomplete="off" /></label>
          <label class="field"><span>Secret Access Key</span><input v-model="secretAccessKey" type="password" autocomplete="new-password" :placeholder="loaded.secretConfigured ? '留空以保留数据库中的现值' : '首次配置必须填写'" /></label>
        </div>
        <label class="toggle-row"><span><strong>Path-style 请求</strong><small>MinIO 通常启用；云厂商按其 S3 兼容说明选择。</small></span><input v-model="pathStyle" type="checkbox" /></label>
      </div>

      <div class="settings-section">
        <h3>公开媒体地址</h3>
        <p>新媒体会把此基础地址与对象 key 拼接并永久写入记录；以后修改不会重写旧 URL。</p>
        <label class="field"><span>Public Base URL</span><input v-model="publicBaseUrl" placeholder="https://media.example.com" autocomplete="off" /></label>
        <label v-if="needsAdoption" class="toggle-row storage-adoption"><span><strong>接管 {{ loaded.legacyAssetCount }} 条历史媒体</strong><small>按当前 Public Base URL 与原 storage key 一次性固化，操作不可由设置更新自动撤销。</small></span><input v-model="adoptLegacyAssets" type="checkbox" /></label>
      </div>

      <div class="settings-section">
        <h3>验证与审计</h3>
        <p>测试会写入一个短探针、从公开 URL 匿名读取，然后删除探针；保存时服务端会再次执行相同测试。</p>
        <label class="field"><span>变更原因</span><textarea v-model="reason" rows="3" minlength="12" placeholder="说明对象存储配置变更的依据与影响" /></label>
        <div v-if="testedUrl" class="storage-tested"><CheckCircle2 :size="16" /><span>最近测试通过：<code>{{ testedUrl }}</code></span></div>
        <div class="storage-actions">
          <button class="button button--secondary" type="button" :disabled="testing || saving" @click="testConnection"><FlaskConical :size="16" />{{ testing ? '测试中…' : '测试连接' }}</button>
          <button class="button button--primary" type="button" :disabled="testing || saving" @click="save"><Save :size="16" />{{ saving ? '保存并复测中…' : '保存设置' }}</button>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
@layer components {
.object-storage-settings { display: grid; gap: 1rem; }
.storage-status { display: flex; align-items: center; gap: .65rem; padding: .75rem; border: 1px solid var(--border-default); border-radius: .6rem; background: var(--surface-subtle); }
.storage-status span { display: grid; gap: .15rem; }.storage-status small { color: var(--text-secondary); font-size: 0.75rem; }
.storage-actions { display: flex; flex-wrap: wrap; gap: .6rem; }.storage-actions button { display: inline-flex; align-items: center; gap: .35rem; }
.storage-tested { display: flex; align-items: flex-start; gap: .4rem; color: #166534; }.storage-tested code { overflow-wrap: anywhere; }
.storage-adoption { border-color: #d97706; }.storage-error { display: flex; align-items: center; justify-content: space-between; gap: 1rem; color: var(--text-danger, #b91c1c); }
}
</style>
