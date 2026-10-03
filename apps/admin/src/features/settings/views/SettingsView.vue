<script setup lang="ts">
import { settingsApi } from '@/features/settings/services/settingsApi'
import { adminIdentityApi } from '@/features/identity'
import { adminAuthApi } from '@/shared/services/adminAuthApi'
import { computed, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Check, Cookie, Copy, Database, Globe2, KeyRound, LockKeyhole, LogOut, Mail, Save, Server, Settings, ShieldCheck, Smartphone, TimerReset } from 'lucide-vue-next'
import DataStatePanel from '@/shared/components/DataStatePanel.vue'
import PageHeader from '@/shared/components/PageHeader.vue'
import ObjectStorageSettingsPanel from '@/features/settings/components/ObjectStorageSettingsPanel.vue'
import IntegrationSettingsPanels from '@/features/settings/components/IntegrationSettingsPanels.vue'
import type { Permission } from '@/shared/types/domain'
import { type AdminSession, type PlatformSettings, type TotpEnrollment, type UpdatePlatformSettings } from '@/shared/services/adminApiTypes'
import { apiErrorMessage, apiProblemStatus } from '@/shared/services/cursorPagination'
import { useAuthStore } from '@/shared/stores/auth'
import { useUiStore } from '@/shared/stores/ui'
import type { ApiProblem } from '@/shared/types/domain'

const route = useRoute()
const router = useRouter()
const auth = useAuthStore()
const ui = useUiStore()
const accountSecurityOnly = route.name === 'account-security'
const active = ref(accountSecurityOnly ? 'security' : String(route.params.section ?? 'general'))
const retentionDays = ref<number | null>(null)
const deletionGraceDays = ref<number | null>(null)
const overrideDefaultDays = ref<number | null>(null)
const changeReason = ref('')
const settingsRevision = ref(0)
const settingsEtag = ref('')
const loadedSettings = ref<PlatformSettings | null>(null)
const settingsLoading = ref(false)
const settingsSaving = ref(false)
const settingsState = ref<'loading' | 'ready' | 'error' | 'forbidden'>(accountSecurityOnly ? 'ready' : 'loading')
const settingsError = ref('')
const enrollment = ref<TotpEnrollment | null>(null)
const confirmationCode = ref('')
const phoneNumber = ref('')
const phonePassword = ref('')
const phoneCode = ref('')
const phoneCodeSent = ref(false)
const phoneBusy = ref(false)
const rotatedRecoveryKey = ref('')
const rotatingRecoveryKey = ref(false)
const sessions = ref<AdminSession[]>([])
const securityLoading = ref(false)

const tabs = [
  { id: 'general', label: '基本设置', icon: Settings },
  { id: 'security', label: '安全与会话', icon: LockKeyhole },
  { id: 'consent', label: 'Consent 与 Analytics', icon: Cookie },
  { id: 'retention', label: '数据保留', icon: TimerReset },
  { id: 'object-storage', label: '对象存储', icon: Database },
  { id: 'mail', label: '邮件投递', icon: Mail, permission: 'mail.manage' as Permission },
  { id: 'sms', label: '短信服务', icon: Smartphone, permission: 'sms.manage' as Permission },
  { id: 'captcha', label: '人机验证', icon: ShieldCheck, permission: 'captcha.manage' as Permission },
  { id: 'domains', label: '域名与 Origin', icon: Globe2 },
]
const visibleTabs = computed(() => {
  const base = accountSecurityOnly ? tabs.filter((tab) => tab.id === 'security') : tabs
  return base.filter((tab) => !tab.permission || auth.hasPermission(tab.permission))
})

const activeTitle = computed(() => tabs.find((tab) => tab.id === active.value)?.label ?? '基本设置')

function selectTab(section: string): void {
  if (accountSecurityOnly) return
  void router.replace({ name: 'settings', params: { section } })
}

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
  settingsState.value = 'loading'
  settingsError.value = ''
  loadedSettings.value = null
  retentionDays.value = null
  deletionGraceDays.value = null
  overrideDefaultDays.value = null
  settingsEtag.value = ''
  try {
    const result = await settingsApi.getSettings()
    applySettings(result.settings, result.etag)
    settingsState.value = 'ready'
  } catch (error) {
    settingsState.value = apiProblemStatus(error) === 403 ? 'forbidden' : 'error'
    settingsError.value = apiErrorMessage(error, '请检查 Settings API 后重试。')
    ui.toast('设置读取失败', settingsError.value, 'danger')
  } finally {
    settingsLoading.value = false
  }
}

async function save(): Promise<void> {
  const baseline = loadedSettings.value
  const retention = retentionDays.value
  const deletionGrace = deletionGraceDays.value
  const overrideDefault = overrideDefaultDays.value
  if (!baseline || settingsState.value !== 'ready') {
    ui.toast('尚未读取设置', '请先等待设置载入后再保存。', 'warning')
    return
  }
  if (retention === null || !Number.isInteger(retention) || retention < 30 || retention > 3650
    || deletionGrace === null || !Number.isInteger(deletionGrace) || deletionGrace < 1 || deletionGrace > 365
    || overrideDefault === null || !Number.isInteger(overrideDefault) || overrideDefault < 1 || overrideDefault > 365) {
    ui.toast('设置值无效', '请按各字段标示的范围输入整数。', 'danger')
    return
  }
  if ([...changeReason.value.trim()].length < 12) {
    ui.toast('需要变更原因', '原因至少填写 12 个字符，并会写入审计记录。', 'warning')
    return
  }
  const payload: UpdatePlatformSettings = { reason: changeReason.value.trim() }
  if (retention !== baseline.rfqRetentionDays) payload.rfqRetentionDays = retention
  if (deletionGrace !== baseline.retentionDeletionGraceDays) payload.retentionDeletionGraceDays = deletionGrace
  if (overrideDefault !== baseline.temporaryOverrideDefaultDays) payload.temporaryOverrideDefaultDays = overrideDefault
  if (Object.keys(payload).length === 1) {
    ui.toast('没有可保存的变更', '三个业务策略值均与当前 revision 相同。', 'info')
    return
  }

  settingsSaving.value = true
  try {
    const result = await settingsApi.updateSettings(payload, settingsRevision.value)
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
  try {
    sessions.value = await adminAuthApi.listSessions()
  } catch (error) {
    ui.toast('会话读取失败', problemMessage(error), 'danger')
  }
}

async function startEnrollment(): Promise<void> {
  securityLoading.value = true
  try {
    enrollment.value = await adminAuthApi.startTotpEnrollment()
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
    await adminAuthApi.confirmTotpEnrollment(confirmationCode.value.trim())
    confirmationCode.value = ''
    enrollment.value = null
    await auth.refresh()
    ui.toast('TOTP 已启用', '该账号登录时将要求输入验证器验证码。', 'success')
  } catch (error) {
    ui.toast('验证码确认失败', problemMessage(error), 'danger')
  } finally {
    securityLoading.value = false
  }
}

async function revokeSession(session: AdminSession): Promise<void> {
  try {
    await adminAuthApi.revokeSession(session.id)
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

async function sendPhoneCode(): Promise<void> {
  phoneBusy.value = true
  try {
    await adminAuthApi.startPhoneVerification(phoneNumber.value.trim(), phonePassword.value)
    phoneCodeSent.value = true
    phonePassword.value = ''
    ui.toast('验证码已发送', '短信验证码 10 分钟内有效，请勿转发给他人。', 'success')
  } catch (error) {
    ui.toast('无法发送验证码', problemMessage(error), 'danger')
  } finally {
    phoneBusy.value = false
  }
}

async function confirmPhoneCode(): Promise<void> {
  phoneBusy.value = true
  try {
    await adminAuthApi.confirmPhoneVerification(phoneCode.value.trim())
    phoneCode.value = ''
    phoneCodeSent.value = false
    phoneNumber.value = ''
    await auth.refresh()
    ui.toast('手机号已绑定', '该号码可用于短信登录与风控验证。', 'success')
  } catch (error) {
    ui.toast('绑定失败', problemMessage(error), 'danger')
  } finally {
    phoneBusy.value = false
  }
}

async function rotateRecoveryKey(): Promise<void> {
  const user = auth.user
  if (!user) return
  rotatingRecoveryKey.value = true
  try {
    const result = await adminIdentityApi.rotateUserRecoveryKey(
      user.id,
      'Rotate the root administrator recovery key from the account security page.',
    )
    rotatedRecoveryKey.value = result.recoveryKey
    ui.toast('恢复密钥已轮换', '新密钥只显示这一次；请保存后完成恢复密钥确认。', 'success')
  } catch (error) {
    ui.toast('轮换失败', problemMessage(error), 'danger')
  } finally {
    rotatingRecoveryKey.value = false
  }
}

async function copyRecoveryKey(): Promise<void> {
  if (rotatedRecoveryKey.value) await navigator.clipboard.writeText(rotatedRecoveryKey.value)
}

async function acknowledgeRotatedKey(): Promise<void> {
  rotatedRecoveryKey.value = ''
  await auth.refresh()
  await router.push({ name: 'onboarding' })
}

function formatTime(value: string): string {
  return new Date(value).toLocaleString('zh-CN')
}

watch(active, (section) => {
  if (section === 'security') void loadSessions()
}, { immediate: true })
watch(() => route.params.section, (value) => {
  if (accountSecurityOnly) return
  const section = typeof value === 'string' && tabs.some((tab) => tab.id === value) ? value : 'general'
  active.value = section
}, { immediate: true })

void loadSettings()
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="PLATFORM CONFIGURATION" :title="accountSecurityOnly ? '账号安全' : '系统设置'" :description="accountSecurityOnly ? '管理本账号的 TOTP、绑定手机号与活动会话。' : '管理平台级配置；对象存储凭据写入数据库但绝不由 API 回显。'">
      <template #actions><button v-if="active === 'retention' && settingsState === 'ready'" class="button button--primary" type="button" :disabled="settingsLoading || settingsSaving || !loadedSettings" @click="save"><Save :size="16" />{{ settingsSaving ? '保存中…' : '保存业务设置' }}</button></template>
    </PageHeader>

    <div v-if="auth.requiresOnboarding" class="security-baseline" role="status">
      <LockKeyhole :size="18" />
      <div><strong>先完成首次安全设置</strong><p>初始密码与管理员恢复密钥确认完成后即可进入管理功能；账号角色已经保留。</p></div>
    </div>

    <div v-if="accountSecurityOnly || settingsState === 'ready'" class="status-banner"><span>{{ active === 'security' ? '实时身份服务' : '实时业务设置' }}</span><p>{{ active === 'security' ? 'TOTP、手机号与会话操作直接调用受认证、CSRF 保护且可审计的 API。' : `三个业务策略值由 Settings API 管理；当前 ${settingsEtag}。部署凭据与 origin 始终只读。` }}</p></div>

    <section class="settings-layout">
      <nav class="settings-nav panel" aria-label="设置导航"><button v-for="tab in visibleTabs" :key="tab.id" type="button" :class="{ 'is-active': active === tab.id }" @click="selectTab(tab.id)"><component :is="tab.icon" :size="17" />{{ tab.label }}</button></nav>

      <DataStatePanel
        v-if="!accountSecurityOnly && settingsState !== 'ready'"
        :state="settingsState"
        :description="settingsState === 'error' ? settingsError : ''"
        @retry="loadSettings"
      />

      <article v-else class="panel settings-panel">
        <header><p class="eyebrow">CONFIGURATION</p><h2>{{ activeTitle }}</h2></header>
        <fieldset class="settings-form" :disabled="!accountSecurityOnly && (settingsLoading || settingsSaving)">

        <template v-if="active === 'general'">
          <div class="settings-section"><h3>站点身份</h3><p>名称与后台语言属于发布基线，不通过业务 Settings API 修改。</p><div class="form-grid"><label class="field"><span>管理平台名称</span><input value="AIRTEKPOWER 管理平台" readonly /></label><label class="field"><span>默认后台语言</span><input value="简体中文" readonly /></label></div></div>
          <div class="settings-section"><h3>内容语言</h3><p>公开站活动 locale 只显示 Settings API 返回值，不使用本地业务默认。</p><label class="toggle-row"><span><strong>{{ loadedSettings?.publicLocale || '未配置' }}</strong><small>服务端配置 · 只读</small></span><input type="checkbox" :checked="Boolean(loadedSettings?.publicLocale)" disabled /></label></div>
        </template>

        <template v-else-if="active === 'security'">
          <div class="settings-section">
            <h3>管理会话</h3>
            <p>会话在连续 60 分钟无活动或创建 24 小时后失效；Cookie 为 host-only、HttpOnly、Secure 与 SameSite=Strict。</p>
            <div class="security-session-list">
              <div v-for="session in sessions" :key="session.id" class="security-session-row">
                <span><strong>{{ session.current ? '当前会话' : '其他会话' }}</strong><small>最近活动 {{ formatTime(session.lastSeenAt) }} · 到期 {{ formatTime(session.expiresAt) }}</small></span>
                <button class="button button--secondary" type="button" @click="revokeSession(session)"><LogOut :size="15" />撤销</button>
              </div>
              <p v-if="!sessions.length" class="inline-note">没有可显示的活动会话。</p>
            </div>
          </div>
          <div class="settings-section">
            <h3>手机号</h3>
            <p>绑定后可用短信验证码登录；触发风控时系统也只向该号码发送验证码。邮箱验证码使用账号邮箱，无需绑定。</p>
            <div v-if="auth.user?.phoneVerified" class="settings-section--success security-status"><Smartphone :size="18" /><div><strong>手机号已绑定并验证</strong><p>换绑需要当前密码与短信验证码。</p></div></div>
            <div class="security-enrollment">
              <label class="field"><span>手机号（E.164）</span><input v-model="phoneNumber" inputmode="tel" autocomplete="tel" placeholder="+8613800138000" /></label>
              <label class="field"><span>当前密码</span><input v-model="phonePassword" type="password" autocomplete="current-password" /></label>
              <button class="button button--primary" type="button" :disabled="phoneBusy || phoneNumber.trim().length < 8 || !phonePassword" @click="sendPhoneCode">{{ phoneCodeSent ? '重新发送验证码' : '发送验证码' }}</button>
              <template v-if="phoneCodeSent">
                <label class="field field--compact"><span>短信验证码</span><input v-model="phoneCode" inputmode="numeric" autocomplete="one-time-code" maxlength="6" /></label>
                <button class="button button--primary" type="button" :disabled="phoneBusy || phoneCode.trim().length !== 6" @click="confirmPhoneCode">确认绑定</button>
              </template>
            </div>
          </div>
          <div class="settings-section">
            <h3>多因素验证</h3>
            <p>标准 TOTP 为 SHA-1、6 位、30 秒，并允许前后一个时间步。TOTP 是可选项；账号登录支持密码、邮箱验证码或已绑定手机号。</p>
            <div v-if="auth.user?.totpEnabled" class="settings-section--success security-status"><ShieldCheck :size="18" /><div><strong>TOTP 已启用</strong><p>登录在密码或验证码之后追加验证器验证码。</p></div></div>
            <button v-else-if="!enrollment" class="button button--primary" type="button" :disabled="securityLoading" @click="startEnrollment"><KeyRound :size="16" />开始绑定 TOTP</button>
            <div v-if="enrollment" class="security-enrollment">
              <label class="field"><span>验证器密钥</span><input :value="enrollment.secret" readonly autocomplete="off" /></label>
              <label class="field"><span>OTPAuth URI</span><textarea :value="enrollment.otpAuthUri" readonly rows="3" autocomplete="off" /></label>
              <label class="field field--compact"><span>当前 6 位验证码</span><input v-model="confirmationCode" inputmode="numeric" autocomplete="one-time-code" maxlength="6" /></label>
              <button class="button button--primary" type="button" :disabled="confirmationCode.length !== 6 || securityLoading" @click="confirmEnrollment">确认并启用</button>
            </div>
          </div>
          <div v-if="auth.user?.roleKeys.includes('super-admin')" class="settings-section">
            <h3>管理员恢复密钥</h3>
            <p>恢复密钥文件是根超管唯一的明文来源，数据库只保存 Argon2 哈希。轮换后旧密钥立即失效，新密钥只显示一次，并需要重新确认。</p>
            <div v-if="rotatedRecoveryKey" class="security-enrollment">
              <label class="field"><span>新的恢复密钥</span><input :value="rotatedRecoveryKey" readonly autocomplete="off" /></label>
              <button class="button button--secondary" type="button" @click="copyRecoveryKey"><Copy :size="15" />复制</button>
              <button class="button button--primary" type="button" @click="acknowledgeRotatedKey">我已保存，去确认</button>
            </div>
            <button v-else class="button button--secondary" type="button" :disabled="rotatingRecoveryKey" @click="rotateRecoveryKey"><KeyRound :size="16" />{{ rotatingRecoveryKey ? '轮换中…' : '轮换我的恢复密钥' }}</button>
          </div>
        </template>

        <template v-else-if="active === 'consent'">
          <div class="settings-section"><h3>生产 Consent 模式</h3><p>生产配置要求严格 opt-in；当前接口未返回运行时 Consent 状态，因此本页不显示推测的开关值。</p><div class="settings-section--success security-status"><ShieldCheck :size="18" /><div><strong>部署规则</strong><p>行为事件与 GA4 必须等待用户同意。</p></div></div></div>
          <div class="settings-section"><h3>Google Analytics 4</h3><p>启用状态、Measurement ID 与 provider 凭据由部署环境或 Secret Provider 注入，后台不回显也不可写。</p><div class="inline-note"><Cookie :size="16" />当前界面不推断 Secret 是否存在或 GA4 是否启用。</div></div>
          <div class="settings-section settings-section--success"><Check :size="18" /><div><strong>PII 白名单规则已启用</strong><p>姓名、邮箱、电话、表单正文与文件名不能进入 Analytics 属性。</p></div></div>
        </template>

        <template v-else-if="active === 'retention'">
          <div class="settings-section"><h3>RFQ 与 Contact</h3><p>到期后进入待删除队列，再清除 PII；两段期限均由同一 revision 原子更新。</p><div class="form-grid"><label class="field field--compact"><span>业务记录保留天数</span><input v-model.number="retentionDays" type="number" min="30" max="3650" step="1" /><small>允许 30–3650 天</small></label><label class="field field--compact"><span>到期后删除宽限天数</span><input v-model.number="deletionGraceDays" type="number" min="1" max="365" step="1" /><small>允许 1–365 天</small></label></div></div>
          <div class="settings-section"><h3>产品临时覆盖</h3><p>新建 Feishu-owned 字段临时覆盖时使用的默认期限；已有覆盖不会被静默改期。</p><label class="field field--compact"><span>默认覆盖天数</span><input v-model.number="overrideDefaultDays" type="number" min="1" max="365" step="1" /><small>允许 1–365 天</small></label></div>
          <div class="settings-section"><h3>变更审计</h3><p>保存必须提供至少 12 个字符的业务原因；服务端记录 actor、before/after、reason 与 request ID。</p><label class="field"><span>变更原因</span><textarea v-model="changeReason" rows="3" minlength="12" placeholder="说明本次策略调整的依据与影响" /></label></div>
          <div class="settings-section"><h3>其他保留策略</h3><p>审计与安全日志等未列入 allowlist 的策略当前只读，不能通过该 API 写入。</p></div>
        </template>

        <template v-else-if="active === 'object-storage'">
          <ObjectStorageSettingsPanel />
        </template>

        <template v-else-if="active === 'mail' || active === 'sms' || active === 'captcha'">
          <IntegrationSettingsPanels :section="active" />
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
