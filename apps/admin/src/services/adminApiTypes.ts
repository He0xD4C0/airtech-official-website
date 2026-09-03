import type {
  AcceptInvitationRequest as ContractAcceptInvitationRequest,
  AdminRoleRecord as ContractAdminRoleRecord,
  AdminProductDetail as ContractAdminProductDetail,
  AdminSession as ContractAdminSession,
  AdminUserRecord as ContractAdminUserRecord,
  AnalyticsOverview as ContractAnalyticsOverview,
  AuditEvent as ContractAuditEvent,
  BackgroundOperation as ContractBackgroundOperation,
  ContentDraftInput as ContractContentDraftInput,
  ContentEntry as ContractContentEntry,
  ContentPreviewLink as ContractContentPreviewLink,
  GeneralInformation as ContractGeneralInformation,
  GeneralInformationPayload as ContractGeneralInformationPayload,
  GeneralInformationRevision as ContractGeneralInformationRevision,
  GuestSourceDaily as ContractGuestSourceDaily,
  GuestVisitAggregate as ContractGuestVisitAggregate,
  InvitationAcceptance as ContractInvitationAcceptance,
  MissingAssetReference as ContractMissingAssetReference,
  NewsDraftInput as ContractNewsDraftInput,
  NewsEntry as ContractNewsEntry,
  NewsRevision as ContractNewsRevision,
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
  SyncConflict as ContractSyncConflict,
  SyncRun as ContractSyncRun,
  TemporaryOverride as ContractTemporaryOverride,
  TotpEnrollment as ContractTotpEnrollment,
  UpdateAdminRole as ContractUpdateAdminRole,
  UpdateAdminUser as ContractUpdateAdminUser,
  UpdatePlatformSettings as ContractUpdatePlatformSettings,
  UpdateProductPresentation as ContractUpdateProductPresentation,
  UserInvitation as ContractUserInvitation,
} from '@airtek/contracts'

export type { CursorPage, CursorPageRequest } from './cursorPagination'

export type BackendContentEntry = ContractContentEntry
export type AcceptInvitationRequest = ContractAcceptInvitationRequest
export type InvitationAcceptance = ContractInvitationAcceptance
export type BackendNewsEntry = ContractNewsEntry
export type NewsDraftPayload = ContractNewsDraftInput

export interface SiteNavigationLink {
  label: string
  href: string
}

export type GeneralInformationPayload = ContractGeneralInformationPayload
export type BackendGeneralInformation = ContractGeneralInformation
export type ProductImportError = ContractProductImportError
export type MissingProductAsset = ContractMissingAssetReference
export type ProductImportResult = ContractProductImportResult
export type ProductImportAccepted = ContractProductImportAccepted
export type GuestVisitAggregate = ContractGuestVisitAggregate
export type GuestSourceDaily = ContractGuestSourceDaily
export type AnalyticsOverview = ContractAnalyticsOverview
export type NewsRevision = ContractNewsRevision
export type GeneralInformationRevision = ContractGeneralInformationRevision
export type AdminUserRecord = ContractAdminUserRecord
export type AdminRoleRecord = ContractAdminRoleRecord
export type ProductPrivatePricing = ContractProductPrivatePricing
export type UpdateAdminUser = ContractUpdateAdminUser
export type UpdateAdminRole = ContractUpdateAdminRole
export type UserInvitation = ContractUserInvitation
export type ContentDraftPayload = ContractContentDraftInput
export type ContentPreviewLink = ContractContentPreviewLink
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
export type BackendSyncConflict = ContractSyncConflict
export type BackendOperation = ContractBackgroundOperation
export type BackendAuditEvent = ContractAuditEvent
export type TotpEnrollment = ContractTotpEnrollment
export type RecoveryCodeSet = ContractRecoveryCodeSet
export type AdminSession = ContractAdminSession
export type PlatformSettings = ContractPlatformSettings
export type UpdatePlatformSettings = ContractUpdatePlatformSettings
