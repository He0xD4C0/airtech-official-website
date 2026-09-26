
import type { AdminDashboardSummary } from '@airtek/contracts'
import { adminContractClient } from '@/shared/services/adminApiTransport'


export const dashboardApi = {
async dashboardSummary(): Promise<AdminDashboardSummary> {
    return (await adminContractClient.get('/api/admin/v1/dashboard/summary', {})).data
  },
}
