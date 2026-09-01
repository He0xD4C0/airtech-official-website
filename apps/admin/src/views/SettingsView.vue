<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Check, Cookie, Copy, Globe2, KeyRound, LockKeyhole, LogOut, Save, Server, Settings, ShieldCheck, TimerReset } from 'lucide-vue-next'
import PageHeader from '@/components/PageHeader.vue'
import { adminApi, mockApiEnabled, type AdminSession, type PlatformSettings, type TotpEnrollment, type UpdatePlatformSettings } from '@/services/adminApi'
import { useAuthStore } from '@/stores/auth'
import { useUiStore } from '@/stores/ui'
import type { ApiProblem } from '@/types/domain'

const route = useRoute()
const router = useRouter()
const auth = useAuthStore()
const ui = useUiStore()
const accountSecurityOnly = route.name === 'account-security'
const active = ref(accountSecurityOnly ? 'security' : String(route.params.section ?? 'general'))
const consentMode = ref<'strict' | 'necessary'>('strict')
const gaEnabled = ref(false)
const retentionDays = ref(365)
const deletionGraceDays = ref(30)
const overrideDefaultDays = ref(30)
const changeReason = ref('')
const settingsRevision = ref(0)
const settingsEtag = ref('')
const loadedSettings = ref<PlatformSettings | null>(null)
const settingsLoading = ref(false)
const settingsSaving = ref(false)
const enrollment = ref<TotpEnrollment | null>(null)
const confirmationCode = ref('')
const recoveryCodes = ref<string[]>([])
const regenerationCode = ref('')
const sessions = ref<AdminSession[]>([])
const securityLoading = ref(false)

const tabs = [
  { id: 'general', label: '基本设置', icon: Settings },
  { id: 'security', label: '安全与会话', icon: LockKeyhole },
  { id: 'consent', label: 'Consent 与 Analytics', icon: Cookie },
  { id: 'retention', label: '数据保留', icon: TimerReset },
  { id: 'domains', label: '域名与 Origin', icon: Globe2 },
]
const visibleTabs = computed(() => accountSecurityOnly ? tabs.filter((tab) => tab.id === 'security') : tabs)

const activeTitle = computed(() => tabs.find((tab) => tab.id === active.value)?.label ?? '基本设置')

function applySettings(settings: PlatformSettings, etag: string): void {
  loadedSettings.value = settings
  retentionDays.value = settings.rfqRetentionDays
  deletionGraceDays.value = settings.retentionDeletionGraceDays
  overrideDefaultDays.value = settings.temporaryOverrideDefaultDays
  settingsRevision.value = settings.revision
  settingsEtag.value = etag
}

async function loadSettings(): Promise<void> {
  if (accountSecurityOnly) return
  settingsLoading.value = true
  try {
    const result = await adminApi.getSettings()
    applySettings(result.settings, result.etag)
  } catch (error) {
    ui.toast('设置读取失败', problemMessage(error), 'danger')
  } finally {
    settingsLoading.value = false
  }
}

async function save(): Promise<void> {
  const baseline = loadedSettings.value
  if (!baseline) {
    ui.toast('尚未读取设置', '请先等待设置载入后再保存。', 'warning')
    return
  }
  if (!Number.isInteger(retentionDays.value) || retentionDays.value < 30 || retentionDays.value > 3650
    || !Number.isInteger(deletionGraceDays.value) || deletionGraceDays.value < 1 || deletionGraceDays.value > 365
    || !Number.isInteger(overrideDefaultDays.value) || overrideDefaultDays.value < 1 || overrideDefaultDays.value > 365) {
    ui.toast('设置值无效', '请按各字段标示的范围输入整数。', 'danger')
    return
  }
  if ([...changeReason.value.trim()].length < 12) {
    ui.toast('需要变更原因', '原因至少填写 12 个字符，并会写入审计记录。', 'warning')
    return
  }
  const payload: UpdatePlatformSettings = { reason: changeReason.value.trim() }
  if (retentionDays.value !== baseline.rfqRetentionDays) payload.rfqRetentionDays = retentionDays.value
  if (deletionGraceDays.value !== baseline.retentionDeletionGraceDays) payload.retentionDeletionGraceDays = deletionGraceDays.value
  if (overrideDefaultDays.value !== baseline.temporaryOverrideDefaultDays) payload.temporaryOverrideDefaultDays = overrideDefaultDays.value
  if (Object.keys(payload).length === 1) {
    ui.toast('没有可保存的变更', '三个业务策略值均与当前 revision 相同。', 'info')
    return
  }

  settingsSaving.value = true
  try {
    const result = await adminApi.updateSettings(payload, settingsRevision.value)
    applySettings(result.settings, result.etag)
    changeReason.value = ''
    ui.toast('业务设置已更新', `已保存 revision ${result.settings.revision}，并记录 before/after 与变更原因。`, 'success')
  } catch (error) {
    const problem = error as ApiProblem
    ui.toast(problem.status === 409 ? '设置发生并发冲突' : '设置保存失败', problemMessage(error), 'danger')
    if (problem.status === 409) await loadSettings()
  } finally {
    settingsSaving.value = false
  }
}

function problemMessage(error: unknown): string {
  const problem = error as ApiProblem
  return problem.detail ?? problem.title ?? '身份服务请求失败。'
}

async function loadSessions(): Promise<void> {
  if (mockApiEnabled) return
  try {
    sessions.value = await adminApi.listSessions()
  } catch (error) {
    ui.toast('会话读取失败', problemMessage(error), 'danger')
  }
}

async function startEnrollment(): Promise<void> {
  securityLoading.value = true
  recoveryCodes.value = []
  try {
    enrollment.value = await adminApi.startTotpEnrollment()
    ui.toast('TOTP 密钥已生成', '请将密钥加入验证器，并用当前验证码完成确认。', 'success')
  } catch (error) {
    ui.toast('无法开始绑定', problemMessage(error), 'danger')
  } finally {
    securityLoading.value = false
  }
}

async function confirmEnrollment(): Promise<void> {
  securityLoading.value = true
  try {
    const result = await adminApi.confirmTotpEnrollment(confirmationCode.value.trim())
    recoveryCodes.value = result.recoveryCodes
    confirmationCode.value = ''
    enrollment.value = null
    await auth.refresh()
    ui.toast('TOTP 已启用', '恢复码只显示这一次，请立即保存到安全位置。', 'success')
  } catch (error) {
    ui.toast('验证码确认失败', problemMessage(error), 'danger')
  } finally {
    securityLoading.value = false
  }
}

async function regenerateCodes(): Promise<void> {
  securityLoading.value = true
  try {
    const result = await adminApi.regenerateRecoveryCodes(regenerationCode.value.trim())
    recoveryCodes.value = result.recoveryCodes
    regenerationCode.value = ''
    ui.toast('恢复码已重新生成', '旧恢复码已全部失效，新码只显示这一次。', 'success')
  } catch (error) {
    ui.toast('恢复码生成失败', problemMessage(error), 'danger')
  } finally {
    securityLoading.value = false
  }
}

async function copyRecoveryCodes(): Promise<void> {
  await navigator.clipboard.writeText(recoveryCodes.value.join('\n'))
  ui.toast('已复制恢复码', '请保存到受保护的密码管理器中。', 'success')
}

async function revokeSession(session: AdminSession): Promise<void> {
  try {
    await adminApi.revokeSession(session.id)
    if (session.current) {
      await auth.logout()
      await router.replace({ name: 'login' })
      return
    }
    await loadSessions()
    ui.toast('会话已撤销', '该会话的 Cookie 已立即失效。', 'success')
  } catch (error) {
    ui.toast('撤销失败', problemMessage(error), 'danger')
  }
}

function formatTime(value: string): string {
  return new Date(value).toLocaleString('zh-CN')
}

watch(active, (section) => {
  if (section === 'security') void loadSessions()
}, { immediate: true })

void loadSettings()
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="PLATFORM CONFIGURATION" :title="accountSecurityOnly ? '账号安全' : '系统设置'" :description="accountSecurityOnly ? '管理本账号的 TOTP、恢复码与活动会话。' : '管理平台级配置；敏感凭据只由后端 Secret Provider 引用，不回显原值。'">
      <template #actions><button v-if="active === 'retention'" class="button button--primary" type="button" :disabled="settingsLoading || settingsSaving || !loadedSettings" @click="save"><Save :size="16" />{{ settingsSaving ? '保存中…' : '保存业务设置' }}</button></template>
    </PageHeader>

    <div class="demo-banner"><span>{{ mockApiEnabled ? '开发演示' : active === 'security' ? '实时身份服务' : '实时业务设置' }}</span><p>{{ mockApiEnabled ? '以下业务设置只验证界面流程，不写入生产配置。' : active === 'security' ? 'TOTP、恢复码与会话操作直接调用受认证、CSRF 保护且可审计的 API。' : `三个业务策略值由 Settings API 管理；当前 ${settingsEtag || 'revision 读取中'}。部署凭据与 origin 始终只读。` }}</p></div>

    <section class="settings-layout">
      <nav class="settings-nav panel" aria-label="设置导航"><button v-for="tab in visibleTabs" :key="tab.id" type="button" :class="{ 'is-active': active === tab.id }" @click="active = tab.id"><component :is="tab.icon" :size="17" />{{ tab.label }}</button></nav>

      <article class="panel settings-panel">
        <header><p class="eyebrow">CONFIGURATION</p><h2>{{ activeTitle }}</h2></header>
        <fieldset class="settings-form" :disabled="!accountSecurityOnly && (settingsLoading || settingsSaving)">

        <template v-if="active === 'general'">
          <div class="settings-section"><h3>站点身份</h3><p>名称与后台语言属于发布基线，不通过业务 Settings API 修改。</p><div class="form-grid"><label class="field"><span>管理平台名称</span><input value="AIRTEKPOWER 管理平台" readonly /></label><label class="field"><span>默认后台语言</span><input value="简体中文" readonly /></label></div></div>
          <div class="settings-section"><h3>内容语言</h3><p>公开站首发为英文；publicLocale 会由 API 返回，但当前固定且不可变。</p><label class="toggle-row"><span><strong>English ({{ loadedSettings?.publicLocale ?? 'en' }})</strong><small>首发语言与唯一活动 locale · 只读</small></span><input type="checkbox" checked disabled /></label></div>
        </template>

        <template v-else-if="active === 'security'">
          <div class="settings-section">
            <h3>管理会话</h3>
            <p>会话在连续 30 分钟无活动或创建 12 小时后失效；Cookie 为 host-only、HttpOnly、Secure 与 SameSite=Strict。</p>
            <div class="security-session-list">
              <div v-for="session in sessions" :key="session.id" class="security-session-row">
                <span><strong>{{ session.current ? '当前会话' : '其他会话' }}</strong><small>最近活动 {{ formatTime(session.lastSeenAt) }} · 到期 {{ formatTime(session.expiresAt) }}</small></span>
                <button class="button button--secondary" type="button" @click="revokeSession(session)"><LogOut :size="15" />撤销</button>
              </div>
              <p v-if="!sessions.length" class="inline-note">{{ mockApiEnabled ? 'Mock 模式不创建真实会话。' : '没有可显示的活动会话。' }}</p>
            </div>
          </div>
          <div class="settings-section">
            <h3>多因素验证</h3>
            <p>标准 TOTP 为 SHA-1、6 位、30 秒，并允许前后一个时间步。高风险数据库任务只接受当前 TOTP，不接受恢复码。</p>
            <div v-if="auth.user?.totpEnabled" class="settings-section--success security-status"><ShieldCheck :size="18" /><div><strong>TOTP 已启用</strong><p>登录可使用 TOTP；恢复码只能使用一次。</p></div></div>
            <button v-else-if="!enrollment" class="button button--primary" type="button" :disabled="mockApiEnabled || securityLoading" @click="startEnrollment"><KeyRound :size="16" />开始绑定 TOTP</button>
            <div v-if="enrollment" class="security-enrollment">
              <label class="field"><span>验证器密钥</span><input :value="enrollment.secret" readonly autocomplete="off" /></label>
              <label class="field"><span>OTPAuth URI</span><textarea :value="enrollment.otpAuthUri" readonly rows="3" autocomplete="off" /></label>
              <label class="field field--compact"><span>当前 6 位验证码</span><input v-model="confirmationCode" inputmode="numeric" autocomplete="one-time-code" maxlength="6" /></label>
              <button class="button button--primary" type="button" :disabled="confirmationCode.length !== 6 || securityLoading" @click="confirmEnrollment">确认并启用</button>
            </div>
            <div v-if="auth.user?.totpEnabled" class="security-regenerate">
              <label class="field field--compact"><span>用当前 TOTP 重新生成恢复码</span><input v-model="regenerationCode" inputmode="numeric" autocomplete="one-time-code" maxlength="6" /></label>
              <button class="button button--secondary" type="button" :disabled="regenerationCode.length !== 6 || securityLoading" @click="regenerateCodes">使旧恢复码失效并生成新码</button>
            </div>
            <div v-if="recoveryCodes.length" class="recovery-code-panel" role="status">
              <div><strong>一次性恢复码</strong><button class="button button--secondary" type="button" @click="copyRecoveryCodes"><Copy :size="15" />复制全部</button></div>
              <p>关闭或刷新页面后不会再次显示。</p>
              <code v-for="code in recoveryCodes" :key="code">{{ code }}</code>
            </div>
          </div>
        </template>

        <template v-else-if="active === 'consent'">
          <div class="settings-section"><h3>生产 Consent 模式</h3><p>生产环境固定为严格 opt-in。该部署策略只读，不属于业务 Settings API。</p><div class="choice-cards"><label :class="{ 'is-active': consentMode === 'strict' }"><input v-model="consentMode" type="radio" value="strict" disabled /><span><ShieldCheck :size="18" /><strong>严格 opt-in</strong><small>行为事件与 GA4 等待同意 · 只读</small></span></label><label :class="{ 'is-active': consentMode === 'necessary' }"><input v-model="consentMode" type="radio" value="necessary" disabled /><span><Cookie :size="18" /><strong>仅必要数据</strong><small>停止全部非必要分析</small></span></label></div></div>
          <div class="settings-section"><h3>Google Analytics 4</h3><p>启用状态、Measurement ID 与 provider 凭据由部署环境或 Secret Provider 注入，后台不回显也不可写。</p><label class="toggle-row"><span><strong>部署级配置</strong><small>只读；当前界面不推断 Secret 是否存在</small></span><input v-model="gaEnabled" type="checkbox" disabled /></label></div>
          <div class="settings-section settings-section--success"><Check :size="18" /><div><strong>PII 白名单规则已启用</strong><p>姓名、邮箱、电话、表单正文与文件名不能进入 Analytics 属性。</p></div></div>
        </template>

        <template v-else-if="active === 'retention'">
          <div class="settings-section"><h3>RFQ 与 Contact</h3><p>到期后进入待删除队列，再清除 PII；两段期限均由同一 revision 原子更新。</p><div class="form-grid"><label class="field field--compact"><span>业务记录保留天数</span><input v-model.number="retentionDays" type="number" min="30" max="3650" step="1" /><small>允许 30–3650 天</small></label><label class="field field--compact"><span>到期后删除宽限天数</span><input v-model.number="deletionGraceDays" type="number" min="1" max="365" step="1" /><small>允许 1–365 天</small></label></div></div>
          <div class="settings-section"><h3>产品临时覆盖</h3><p>新建 Feishu-owned 字段临时覆盖时使用的默认期限；已有覆盖不会被静默改期。</p><label class="field field--compact"><span>默认覆盖天数</span><input v-model.number="overrideDefaultDays" type="number" min="1" max="365" step="1" /><small>允许 1–365 天</small></label></div>
          <div class="settings-section"><h3>变更审计</h3><p>保存必须提供至少 12 个字符的业务原因；服务端记录 actor、before/after、reason 与 request ID。</p><label class="field"><span>变更原因</span><textarea v-model="changeReason" rows="3" minlength="12" placeholder="说明本次策略调整的依据与影响" /></label></div>
          <div class="settings-section"><h3>其他保留策略</h3><p>审计与安全日志等未列入 allowlist 的策略当前只读，不能通过该 API 写入。</p></div>
        </template>

        <template v-else>
          <div class="settings-section"><h3>Origin 隔离</h3><p>三个 origin 分别构建和部署；域名、端口与 Cookie 策略来自部署环境，只读且不由 Settings API 接受。</p><div class="origin-list"><div><Globe2 :size="17" /><span><strong>Public Web</strong><code>https://www.&lt;domain&gt;</code></span><em>:3000 internal</em></div><div><ShieldCheck :size="17" /><span><strong>Admin Web</strong><code>https://admin.&lt;domain&gt;</code></span><em>:3100 internal</em></div><div><Server :size="17" /><span><strong>Rust API</strong><code>https://api.&lt;domain&gt;</code></span><em>:8080 internal</em></div></div></div>
          <div class="settings-section"><h3>抓取隔离</h3><ul class="check-list"><li><Check :size="15" />Admin robots.txt: Disallow /</li><li><Check :size="15" />Admin Sitemap 路由返回 404</li><li><Check :size="15" />所有 Admin 响应包含 X-Robots-Tag</li></ul></div>
        </template>
        </fieldset>
      </article>
    </section>
  </div>
</template>
