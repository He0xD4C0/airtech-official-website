import type {
  AcceptInvitationRequest as ContractAcceptInvitationRequest,
  AdminRoleRecord as ContractAdminRoleRecord,
  AdminProductDetail as ContractAdminProductDetail,
  AdminSession as ContractAdminSession,
  AdminUserRecord as ContractAdminUserRecord,
  AnalyticsOverview as ContractAnalyticsOverview,
  AuditEvent as ContractAuditEvent,
  BackgroundOperation as ContractBackgroundOperation,
  GuestSourceDaily as ContractGuestSourceDaily,
  InvitationAcceptance as ContractInvitationAcceptance,
  MissingAssetReference as ContractMissingAssetReference,
  ObjectStorageSettings as ContractObjectStorageSettings,
  ObjectStorageSettingsInput as ContractObjectStorageSettingsInput,
  ObjectStorageTestResult as ContractObjectStorageTestResult,
  PerformanceCurve as ContractPerformanceCurve,
  PerformancePoint as ContractPerformancePoint,
  PlatformSettings as ContractPlatformSettings,
  Product as ContractProduct,
  ProductImportAccepted as ContractProductImportAccepted,
  ProductImportError as ContractProductImportError,
  ProductImportResult as ContractProductImportResult,
  ProductPrivatePricing as ContractProductPrivatePricing,
  RecoveryCodeSet as ContractRecoveryCodeSet,
  SpecValue as ContractSpecValue,
  SyncRun as ContractSyncRun,
  TemporaryOverride as ContractTemporaryOverride,
  TotpEnrollment as ContractTotpEnrollment,
  UpdateAdminRole as ContractUpdateAdminRole,
  UpdateAdminUser as ContractUpdateAdminUser,
  UpdateObjectStorageSettings as ContractUpdateObjectStorageSettings,
  UpdatePlatformSettings as ContractUpdatePlatformSettings,
  UpdateProductPresentation as ContractUpdateProductPresentation,
  UserInvitation as ContractUserInvitation,
} from '@airtek/contracts'

export type { CursorPage, CursorPageRequest } from './cursorPagination'

export type AcceptInvitationRequest = ContractAcceptInvitationRequest
export type InvitationAcceptance = ContractInvitationAcceptance

export type ProductImportError = ContractProductImportError
export type MissingProductAsset = ContractMissingAssetReference
export type ProductImportResult = ContractProductImportResult
export type ProductImportAccepted = ContractProductImportAccepted
export type GuestSourceDaily = ContractGuestSourceDaily
export type AnalyticsOverview = ContractAnalyticsOverview
export type AdminUserRecord = ContractAdminUserRecord
export type AdminRoleRecord = ContractAdminRoleRecord
export type ProductPrivatePricing = ContractProductPrivatePricing
export type UpdateAdminUser = ContractUpdateAdminUser
export type UpdateAdminRole = ContractUpdateAdminRole
export type UserInvitation = ContractUserInvitation
export type BackendProduct = ContractProduct & {
  sourceKind?: ContractAdminProductDetail['sourceKind']
  missingAssets?: ContractAdminProductDetail['missingAssets']
  presentation?: ContractAdminProductDetail['presentation']
}
export type ProductPresentationPayload = ContractUpdateProductPresentation
export type BackendSpecValue = ContractSpecValue
export type BackendPerformancePoint = ContractPerformancePoint
export type BackendPerformanceCurve = ContractPerformanceCurve
export type BackendTemporaryOverride = ContractTemporaryOverride
export type BackendSyncRun = ContractSyncRun
export type BackendOperation = ContractBackgroundOperation
export type BackendAuditEvent = ContractAuditEvent
export type TotpEnrollment = ContractTotpEnrollment
export type RecoveryCodeSet = ContractRecoveryCodeSet
export type AdminSession = ContractAdminSession
export type PlatformSettings = ContractPlatformSettings
export type UpdatePlatformSettings = ContractUpdatePlatformSettings
export type ObjectStorageSettings = ContractObjectStorageSettings
export type ObjectStorageSettingsInput = ContractObjectStorageSettingsInput
export type ObjectStorageTestResult = ContractObjectStorageTestResult
export type UpdateObjectStorageSettings = ContractUpdateObjectStorageSettings
