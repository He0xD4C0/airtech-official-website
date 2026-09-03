import { adminAuthApi } from './adminAuthApi'
import { adminContentApi } from './adminContentApi'
import { adminEngagementApi } from './adminEngagementApi'
import { adminIdentityApi } from './adminIdentityApi'
import { adminIntegrationApi } from './adminIntegrationApi'
import { adminOperationsApi } from './adminOperationsApi'
import { adminProductApi } from './adminProductApi'

export * from './adminApiTypes'
export { productImportResult, waitForOperation } from './adminOperationsApi'

export const adminApi = {
  ...adminAuthApi,
  ...adminContentApi,
  ...adminProductApi,
  ...adminIntegrationApi,
  ...adminEngagementApi,
  ...adminIdentityApi,
  ...adminOperationsApi,
}
