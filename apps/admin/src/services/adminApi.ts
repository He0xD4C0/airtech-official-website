import { adminAuthApi } from './adminAuthApi'
import { adminEngagementApi } from './adminEngagementApi'
import { adminIdentityApi } from './adminIdentityApi'
import { adminIntegrationApi } from './adminIntegrationApi'
import { adminOperationsApi } from './adminOperationsApi'
import { adminProductApi } from './adminProductApi'

export * from './adminApiTypes'
export { productImportResult, waitForOperation } from './adminOperationsApi'

export const adminApi = {
  ...adminAuthApi,
  ...adminProductApi,
  ...adminIntegrationApi,
  ...adminEngagementApi,
  ...adminIdentityApi,
  ...adminOperationsApi,
}
