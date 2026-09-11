import { collectCursorPages, findInCursorPages } from './cursorPagination'
import { adminContractClient, cursorQuery, randomRequestId, revisionEtag } from './adminApiTransport'
import type {
  BackendProduct,
  BackendTemporaryOverride,
  CursorPage,
  CursorPageRequest,
  ProductImportAccepted,
  ProductImportResult,
  ProductPresentationPayload,
  ProductPrivatePricing,
} from './adminApiTypes'

export interface ProductListRequest extends CursorPageRequest {
  q?: string
}

export const adminProductApi = {
  async listProducts(request: ProductListRequest = {}): Promise<CursorPage<BackendProduct>> {
    const result = await adminContractClient.get('/api/admin/v1/products', {
      parameters: {
        query: {
          ...cursorQuery(request),
          ...(request.q?.trim() ? { q: request.q.trim() } : {}),
        },
      },
    })
    return result.data
  },

  async getProduct(id: string): Promise<BackendProduct> {
    const result = await adminContractClient.get('/api/admin/v1/products/{id}', { parameters: { path: { id } } })
    return result.data
  },

  findProduct: (id: string) => findInCursorPages(
    async (pagination) => {
      const result = await adminContractClient.get('/api/admin/v1/products', {
        parameters: { query: cursorQuery(pagination) },
      })
      return result.data
    },
    (product) => product.id === id,
  ),

  async updateProductPresentation(id: string, revision: number, payload: ProductPresentationPayload): Promise<{ product: BackendProduct; etag: string }> {
    const result = await adminContractClient.patch('/api/admin/v1/products/{id}/presentation', {
      parameters: { path: { id }, header: { 'If-Match': revisionEtag(revision), 'Idempotency-Key': randomRequestId() } },
      body: payload,
    })
    return { product: result.data, etag: result.etag ?? '' }
  },

  async getProductPrivatePricing(id: string): Promise<ProductPrivatePricing> {
    const result = await adminContractClient.get('/api/admin/v1/products/{id}/private-pricing', {
      parameters: { path: { id } },
      cache: 'no-store',
    })
    return result.data
  },

  async listTemporaryOverrides(productId: string, pagination?: CursorPageRequest): Promise<CursorPage<BackendTemporaryOverride>> {
    const result = await adminContractClient.get('/api/admin/v1/products/{id}/temporary-overrides', {
      parameters: { path: { id: productId }, query: cursorQuery(pagination) },
    })
    return result.data
  },

  listAllTemporaryOverrides: (productId: string) => collectCursorPages(
    async (pagination) => {
      const result = await adminContractClient.get('/api/admin/v1/products/{id}/temporary-overrides', {
        parameters: { path: { id: productId }, query: cursorQuery(pagination) },
      })
      return result.data
    },
  ),

  async publishProduct(id: string, revision: number, idempotencyKey: string = randomRequestId()): Promise<{ product: BackendProduct; etag: string }> {
    const result = await adminContractClient.post('/api/admin/v1/products/{id}/publish', {
      parameters: {
        path: { id },
        header: { 'Idempotency-Key': idempotencyKey, 'If-Match': revisionEtag(revision) },
      },
    })
    return { product: result.data, etag: result.etag ?? '' }
  },

  async listProductImports(pagination?: CursorPageRequest): Promise<CursorPage<ProductImportResult>> {
    const result = await adminContractClient.get('/api/admin/v1/products/imports', {
      parameters: { query: cursorQuery(pagination) },
    })
    return result.data
  },

  async importProductMaster(csv: string, mappingVersion?: string): Promise<ProductImportAccepted> {
    const result = await adminContractClient.post('/api/admin/v1/products/imports', {
      parameters: { header: { 'Idempotency-Key': randomRequestId() } },
      body: mappingVersion ? { csv, mappingVersion } : { csv },
    })
    return result.data
  },
}
