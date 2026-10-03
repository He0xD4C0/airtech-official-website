import { adminContractClient, revisionEtag } from '@/shared/services/adminApiTransport'
import type {
  CaptchaSettings,
  IntegrationTestResult,
  MailSettings,
  SmsSettings,
  UpdateCaptchaSettings,
  UpdateMailSettings,
  UpdateSmsSettings,
} from '@airtek/contracts'
import type { ObjectStorageSettings, ObjectStorageSettingsInput, ObjectStorageTestResult, PlatformSettings, UpdatePlatformSettings, UpdateObjectStorageSettings } from '@/shared/services/adminApiTypes'


export const settingsApi = {
async getSettings(): Promise<{ settings: PlatformSettings; etag: string }> {
    const result = await adminContractClient.get('/api/admin/v1/settings')
    return { settings: result.data, etag: result.etag ?? '' }
  },

async updateSettings(payload: UpdatePlatformSettings, revision: number): Promise<{ settings: PlatformSettings; etag: string }> {
    const result = await adminContractClient.patch('/api/admin/v1/settings', {
      parameters: { header: { 'If-Match': revisionEtag(revision) } },
      body: payload,
    })
    return { settings: result.data, etag: result.etag ?? '' }
  },

async getObjectStorageSettings(): Promise<{ settings: ObjectStorageSettings; etag: string }> {
    const result = await adminContractClient.get('/api/admin/v1/settings/object-storage')
    return { settings: result.data, etag: result.etag ?? '' }
  },

async testObjectStorageSettings(payload: ObjectStorageSettingsInput): Promise<ObjectStorageTestResult> {
    const result = await adminContractClient.post('/api/admin/v1/settings/object-storage/test', {
      body: payload,
    })
    return result.data
  },

  async updateObjectStorageSettings(
    payload: UpdateObjectStorageSettings,
    revision: number,
  ): Promise<{ settings: ObjectStorageSettings; etag: string }> {
    const result = await adminContractClient.put('/api/admin/v1/settings/object-storage', {
      parameters: { header: { 'If-Match': revisionEtag(revision, true) } },
      body: payload,
    })
    return { settings: result.data, etag: result.etag ?? '' }
  },

  async getMailSettings(): Promise<{ settings: MailSettings; etag: string }> {
    const result = await adminContractClient.get('/api/admin/v1/settings/mail')
    return { settings: result.data, etag: result.etag ?? '' }
  },

  async updateMailSettings(payload: UpdateMailSettings, revision: number): Promise<{ settings: MailSettings; etag: string }> {
    const result = await adminContractClient.put('/api/admin/v1/settings/mail', {
      parameters: { header: { 'If-Match': revisionEtag(revision, true) } },
      body: payload,
    })
    return { settings: result.data, etag: result.etag ?? '' }
  },

  async testMailSettings(payload: { to?: string }): Promise<IntegrationTestResult> {
    const result = await adminContractClient.post('/api/admin/v1/settings/mail/test', { body: payload })
    return result.data
  },

  async getSmsSettings(): Promise<{ settings: SmsSettings; etag: string }> {
    const result = await adminContractClient.get('/api/admin/v1/settings/sms')
    return { settings: result.data, etag: result.etag ?? '' }
  },

  async updateSmsSettings(payload: UpdateSmsSettings, revision: number): Promise<{ settings: SmsSettings; etag: string }> {
    const result = await adminContractClient.put('/api/admin/v1/settings/sms', {
      parameters: { header: { 'If-Match': revisionEtag(revision, true) } },
      body: payload,
    })
    return { settings: result.data, etag: result.etag ?? '' }
  },

  async testSmsSettings(payload: { phone: string }): Promise<IntegrationTestResult> {
    const result = await adminContractClient.post('/api/admin/v1/settings/sms/test', { body: payload })
    return result.data
  },

  async getCaptchaSettings(): Promise<{ settings: CaptchaSettings; etag: string }> {
    const result = await adminContractClient.get('/api/admin/v1/settings/captcha')
    return { settings: result.data, etag: result.etag ?? '' }
  },

  async updateCaptchaSettings(payload: UpdateCaptchaSettings, revision: number): Promise<{ settings: CaptchaSettings; etag: string }> {
    const result = await adminContractClient.put('/api/admin/v1/settings/captcha', {
      parameters: { header: { 'If-Match': revisionEtag(revision, true) } },
      body: payload,
    })
    return { settings: result.data, etag: result.etag ?? '' }
  },

  async testCaptchaSettings(payload: { token: string }): Promise<IntegrationTestResult> {
    const result = await adminContractClient.post('/api/admin/v1/settings/captcha/test', { body: payload })
    return result.data
  },
}
