<script setup lang="ts">
import { onMounted, reactive, ref, watch } from 'vue'
import { Save, Send, ShieldCheck } from 'lucide-vue-next'
import { settingsApi } from '@/features/settings/services/settingsApi'
import { useUiStore } from '@/shared/stores/ui'
import type { ApiProblem } from '@/shared/types/domain'

const props = defineProps<{ section: 'mail' | 'sms' | 'captcha' }>()
const ui = useUiStore()

const loading = ref(false)
const saving = ref(false)
const testing = ref(false)
const revision = ref(0)
const configured = ref(false)
const reason = ref('')
const testTarget = ref('')
const testToken = ref('')

const mail = reactive<{
  host: string
  port: number
  protocol: 'starttls' | 'tls' | 'plain'
  username: string
  password: string
  fromAddress: string
  fromName: string
}>({
  host: '',
  port: 587,
  protocol: 'starttls',
  username: '',
  password: '',
  fromAddress: '',
  fromName: '',
})
const sms = reactive<{
  provider: 'aliyun'
  accessKeyId: string
  accessKeySecret: string
  signName: string
  templateCode: string
  region: string
}>({
  provider: 'aliyun',
  accessKeyId: '',
  accessKeySecret: '',
  signName: '',
  templateCode: '',
  region: 'cn-hangzhou',
})
const captcha = reactive<{
  provider: 'turnstile' | 'recaptcha' | 'hcaptcha'
  siteKey: string
  secretKey: string
}>({
  provider: 'turnstile',
  siteKey: '',
  secretKey: '',
})

async function load(): Promise<void> {
  loading.value = true
  try {
    if (props.section === 'mail') {
      const { settings } = await settingsApi.getMailSettings()
      Object.assign(mail, {
        host: settings.host,
        port: settings.port,
        protocol: settings.protocol,
        username: settings.username,
        password: '',
        fromAddress: settings.fromAddress,
        fromName: settings.fromName,
      })
      revision.value = settings.revision
      configured.value = settings.configured
    } else if (props.section === 'sms') {
      const { settings } = await settingsApi.getSmsSettings()
      Object.assign(sms, {
        provider: settings.provider,
        accessKeyId: settings.accessKeyId,
        accessKeySecret: '',
        signName: settings.signName,
        templateCode: settings.templateCode,
        region: settings.region,
      })
      revision.value = settings.revision
      configured.value = settings.configured
    } else {
      const { settings } = await settingsApi.getCaptchaSettings()
      Object.assign(captcha, {
        provider: settings.provider,
        siteKey: settings.siteKey,
        secretKey: '',
      })
      revision.value = settings.revision
      configured.value = settings.configured
    }
  } catch (error) {
    ui.toast('读取失败', message(error), 'danger')
  } finally {
    loading.value = false
  }
}

function requireReason(): boolean {
  if (reason.value.trim().length < 10) {
    ui.toast('需要变更原因', '至少 10 个字符，将写入审计日志。', 'warning')
    return false
  }
  return true
}

function message(error: unknown): string {
  return (error as ApiProblem).detail ?? (error as ApiProblem).title ?? '操作失败，请重试。'
}

async function save(): Promise<void> {
  if (!requireReason()) return
  saving.value = true
  try {
    if (props.section === 'mail') {
      const { settings } = await settingsApi.updateMailSettings(
        {
          host: mail.host,
          port: Number(mail.port),
          protocol: mail.protocol,
          ...(mail.username ? { username: mail.username } : {}),
          ...(mail.password ? { password: mail.password } : {}),
          fromAddress: mail.fromAddress,
          fromName: mail.fromName,
          reason: reason.value,
        },
        revision.value,
      )
      revision.value = settings.revision
      configured.value = settings.configured
      mail.password = ''
    } else if (props.section === 'sms') {
      const { settings } = await settingsApi.updateSmsSettings(
        {
          provider: sms.provider,
          ...(sms.accessKeyId ? { accessKeyId: sms.accessKeyId } : {}),
          ...(sms.accessKeySecret ? { accessKeySecret: sms.accessKeySecret } : {}),
          signName: sms.signName,
          templateCode: sms.templateCode,
          region: sms.region,
          reason: reason.value,
        },
        revision.value,
      )
      revision.value = settings.revision
      configured.value = settings.configured
      sms.accessKeySecret = ''
    } else {
      const { settings } = await settingsApi.updateCaptchaSettings(
        {
          provider: captcha.provider,
          siteKey: captcha.siteKey,
          ...(captcha.secretKey ? { secretKey: captcha.secretKey } : {}),
          reason: reason.value,
        },
        revision.value,
      )
      revision.value = settings.revision
      configured.value = settings.configured
      captcha.secretKey = ''
    }
    reason.value = ''
    ui.toast('已保存', '配置已写入并记录审计。', 'success')
  } catch (error) {
    ui.toast('保存失败', message(error), 'danger')
  } finally {
    saving.value = false
  }
}

async function test(): Promise<void> {
  testing.value = true
  try {
    if (props.section === 'mail') {
      const result = await settingsApi.testMailSettings(testTarget.value ? { to: testTarget.value } : {})
      ui.toast(result.delivered ? '测试邮件已发送' : '测试未发送', result.delivered ? '请检查收件箱。' : '配置不完整。', result.delivered ? 'success' : 'warning')
    } else if (props.section === 'sms') {
      const result = await settingsApi.testSmsSettings({ phone: testTarget.value.trim() })
      ui.toast(result.delivered ? '测试短信已发送' : '测试未发送', result.delivered ? '请检查手机。' : '配置不完整。', result.delivered ? 'success' : 'warning')
    } else {
      const result = await settingsApi.testCaptchaSettings({ token: testToken.value.trim() })
      ui.toast(result.verified ? 'CAPTCHA 校验通过' : 'CAPTCHA 校验未通过', result.verified ? '令牌有效。' : '请更换令牌重试。', result.verified ? 'success' : 'warning')
    }
  } catch (error) {
    ui.toast('测试失败', message(error), 'danger')
  } finally {
    testing.value = false
  }
}

onMounted(load)
watch(() => props.section, load)
</script>

<template>
  <div class="settings-section" v-if="!loading">
    <div class="status-banner">
      <span>{{ configured ? '已配置' : '未配置' }}</span>
      <p>密钥字段只写不回显；留空表示保持现有值。Revision {{ revision }}。</p>
    </div>

    <template v-if="section === 'mail'">
      <div class="form-grid">
        <label class="field"><span>SMTP 主机</span><input v-model="mail.host" /></label>
        <label class="field"><span>端口</span><input v-model.number="mail.port" type="number" min="1" max="65535" /></label>
        <label class="field"><span>加密协议</span><select v-model="mail.protocol"><option value="starttls">STARTTLS</option><option value="tls">TLS</option><option value="plain">明文（仅限本地中继，禁止认证）</option></select></label>
        <label class="field"><span>用户名</span><input v-model="mail.username" autocomplete="off" /></label>
        <label class="field"><span>密码 / 授权码</span><input v-model="mail.password" type="password" autocomplete="new-password" /></label>
        <label class="field"><span>发件地址</span><input v-model="mail.fromAddress" type="email" /></label>
        <label class="field"><span>发件人名称</span><input v-model="mail.fromName" /></label>
      </div>
      <p class="inline-note">STARTTLS/TLS 可携带账号密码；明文只允许无凭据的受信本地中继，明文认证会被服务端拒绝。</p>
      <label class="field field--compact"><span>测试收件地址（留空发给当前账号）</span><input v-model="testTarget" type="email" /></label>
    </template>

    <template v-else-if="section === 'sms'">
      <div class="form-grid">
        <label class="field"><span>服务商</span><select v-model="sms.provider"><option value="aliyun">阿里云短信</option></select></label>
        <label class="field"><span>AccessKeyId</span><input v-model="sms.accessKeyId" autocomplete="off" /></label>
        <label class="field"><span>AccessKeySecret</span><input v-model="sms.accessKeySecret" type="password" autocomplete="new-password" /></label>
        <label class="field"><span>签名名称</span><input v-model="sms.signName" /></label>
        <label class="field"><span>模板 Code</span><input v-model="sms.templateCode" /></label>
        <label class="field"><span>区域</span><input v-model="sms.region" /></label>
      </div>
      <p class="inline-note">签名与模板需要先通过运营商审核，审核周期请预留。</p>
      <label class="field field--compact"><span>测试手机号（E.164）</span><input v-model="testTarget" placeholder="+8613800000000" /></label>
    </template>

    <template v-else>
      <div class="form-grid">
        <label class="field"><span>服务商</span><select v-model="captcha.provider"><option value="turnstile">Cloudflare Turnstile</option><option value="recaptcha">Google reCAPTCHA</option><option value="hcaptcha">hCaptcha</option></select></label>
        <label class="field"><span>Site Key</span><input v-model="captcha.siteKey" autocomplete="off" /></label>
        <label class="field"><span>Secret Key</span><input v-model="captcha.secretKey" type="password" autocomplete="new-password" /></label>
      </div>
      <p class="inline-note">未配置或服务不可达时的行为由部署变量 AIRTEK_CAPTCHA_FAILURE_MODE 决定。</p>
      <label class="field field--compact"><span>测试令牌</span><input v-model="testToken" autocomplete="off" /></label>
    </template>

    <label class="field field--compact"><span>变更原因</span><textarea v-model="reason" rows="2" placeholder="至少 10 个字符，将写入审计。"></textarea></label>
    <div class="settings-section--actions">
      <button class="button button--primary" type="button" :disabled="saving" @click="save"><Save :size="16" />{{ saving ? '保存中…' : '保存配置' }}</button>
      <button class="button button--secondary" type="button" :disabled="testing" @click="test"><Send :size="15" />{{ testing ? '测试中…' : '测试' }}</button>
      <span class="inline-note"><ShieldCheck :size="14" /> 密钥不会通过 API 回显</span>
    </div>
  </div>
</template>
