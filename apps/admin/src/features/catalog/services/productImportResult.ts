import type { BackendOperation, ProductImportResult } from '@/shared/services/adminApiTypes'

export function productImportResult(operation: BackendOperation): ProductImportResult {
  if (operation.status === 'failed') throw new Error('Product Master import failed. Review the operation audit record.')
  const value = operation.result
  if (!value || typeof value !== 'object' || !('import' in value)) {
    throw new Error('The completed operation did not contain a Product Master import report.')
  }
  return (value as { import: ProductImportResult }).import
}

