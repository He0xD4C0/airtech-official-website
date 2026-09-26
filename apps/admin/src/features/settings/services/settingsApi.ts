import { adminContractClient, revisionEtag } from '@/shared/services/adminApiTransport'
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
}
