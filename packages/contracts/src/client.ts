import type { paths, ProblemDetails } from './generated/openapi'

export type ContractHttpMethod = 'get' | 'put' | 'post' | 'delete' | 'options' | 'head' | 'patch' | 'trace'

type Defined<T> = Exclude<T, undefined>
type IsNever<T> = [T] extends [never] ? true : false

export type ContractPath<Method extends ContractHttpMethod> = {
  [Path in keyof paths]: Method extends keyof paths[Path]
    ? IsNever<Defined<paths[Path][Method]>> extends true ? never : Path
    : never
}[keyof paths] & string

export type ContractOperation<
  Method extends ContractHttpMethod,
  Path extends ContractPath<Method>,
> = Method extends keyof paths[Path] ? Defined<paths[Path][Method]> : never

type RequestMediaBody<RequestBody> = RequestBody extends { content: infer Content }
  ? Content extends { 'application/json': infer Json } ? Json
    : Content extends Record<string, unknown> ? Content[keyof Content]
      : never
  : never

export type ContractRequestBody<Operation> = Operation extends { requestBody: infer RequestBody }
  ? RequestMediaBody<RequestBody>
  : Operation extends { requestBody?: infer RequestBody }
    ? RequestMediaBody<Defined<RequestBody>>
    : never

type SuccessStatus =
  | 200 | 201 | 202 | 203 | 204 | 205 | 206 | 207 | 208 | 226
  | '200' | '201' | '202' | '203' | '204' | '205' | '206' | '207' | '208' | '226'

type ResponseMediaBody<Response> = Response extends { content: infer Content }
  ? Content extends Record<string, unknown> ? Content[keyof Content] : never
  : undefined

export type ContractSuccessBody<Operation> = Operation extends { responses: infer Responses }
  ? {
      [Status in keyof Responses]: Status extends SuccessStatus
        ? ResponseMediaBody<Responses[Status]>
        : never
    }[keyof Responses]
  : never

type ParameterPart<Parameters, Name extends 'query' | 'header' | 'path' | 'cookie'> =
  Parameters extends Record<Name, infer Value>
    ? { [Key in Name]: Defined<Value> }
    : Name extends keyof Parameters
      ? IsNever<Defined<Parameters[Name]>> extends true
        ? Record<never, never>
        : { [Key in Name]?: Defined<Parameters[Name]> }
      : Record<never, never>

type ContractParameterBag<Operation> = Operation extends { parameters: infer Parameters }
  ? ParameterPart<Parameters, 'path'>
    & ParameterPart<Parameters, 'query'>
    & ParameterPart<Parameters, 'header'>
    & ParameterPart<Parameters, 'cookie'>
  : Record<never, never>

type ContractBodyOption<Operation> = Operation extends { requestBody: infer RequestBody }
  ? { body: RequestMediaBody<RequestBody> }
  : { body?: ContractRequestBody<Operation> }

export type ContractRequestOptions<Operation> = Omit<
  RequestInit,
  'method' | 'body' | 'headers'
> & ContractBodyOption<Operation> & ContractParameterOption<Operation> & {
  headers?: HeadersInit
}

type HasRequiredParameters<Operation> = Operation extends { parameters: infer Parameters }
  ? Parameters extends { path: unknown } ? true
    : Parameters extends { query: unknown } ? true
      : Parameters extends { header: unknown } ? true
        : Parameters extends { cookie: unknown } ? true
          : false
  : false

type ContractParameterOption<Operation> = HasRequiredParameters<Operation> extends true
  ? { parameters: ContractParameterBag<Operation> }
  : keyof ContractParameterBag<Operation> extends never
    ? { parameters?: never }
    : { parameters?: ContractParameterBag<Operation> }

type ContractOptionsTuple<Operation> = Operation extends { requestBody: unknown }
  ? [options: ContractRequestOptions<Operation>]
  : HasRequiredParameters<Operation> extends true
    ? [options: ContractRequestOptions<Operation>]
    : [options?: ContractRequestOptions<Operation>]

export interface ContractResponse<Data> {
  data: Data
  response: Response
  etag: string | null
}

export interface ContractClient {
  request<Method extends ContractHttpMethod, Path extends ContractPath<Method>>(
    method: Method,
    path: Path,
    ...options: ContractOptionsTuple<ContractOperation<Method, Path>>
  ): Promise<ContractResponse<ContractSuccessBody<ContractOperation<Method, Path>>>>

  raw<Method extends ContractHttpMethod, Path extends ContractPath<Method>>(
    method: Method,
    path: Path,
    ...options: ContractOptionsTuple<ContractOperation<Method, Path>>
  ): Promise<Response>

  get<Path extends ContractPath<'get'>>(
    path: Path,
    ...options: ContractOptionsTuple<ContractOperation<'get', Path>>
  ): Promise<ContractResponse<ContractSuccessBody<ContractOperation<'get', Path>>>>

  post<Path extends ContractPath<'post'>>(
    path: Path,
    ...options: ContractOptionsTuple<ContractOperation<'post', Path>>
  ): Promise<ContractResponse<ContractSuccessBody<ContractOperation<'post', Path>>>>

  put<Path extends ContractPath<'put'>>(
    path: Path,
    ...options: ContractOptionsTuple<ContractOperation<'put', Path>>
  ): Promise<ContractResponse<ContractSuccessBody<ContractOperation<'put', Path>>>>

  patch<Path extends ContractPath<'patch'>>(
    path: Path,
    ...options: ContractOptionsTuple<ContractOperation<'patch', Path>>
  ): Promise<ContractResponse<ContractSuccessBody<ContractOperation<'patch', Path>>>>

  delete<Path extends ContractPath<'delete'>>(
    path: Path,
    ...options: ContractOptionsTuple<ContractOperation<'delete', Path>>
  ): Promise<ContractResponse<ContractSuccessBody<ContractOperation<'delete', Path>>>>
}

export class ApiError extends Error {
  public readonly type: ProblemDetails['type']
  public readonly title: ProblemDetails['title']
  public readonly status: ProblemDetails['status']
  public readonly detail: ProblemDetails['detail']
  public readonly instance: ProblemDetails['instance']
  public readonly errors: ProblemDetails['errors']
  public readonly requestId: ProblemDetails['requestId']

  constructor(
    public readonly problem: ProblemDetails,
    public readonly response?: Response,
  ) {
    super(problem.detail || problem.title)
    this.name = 'ApiError'
    this.type = problem.type
    this.title = problem.title
    this.status = problem.status
    this.detail = problem.detail
    this.instance = problem.instance
    this.errors = problem.errors
    this.requestId = problem.requestId
  }
}

export interface ApiClientOptions {
  baseUrl: string
  fetchImpl?: typeof fetch
  getCsrfToken?: () => string | undefined
  headers?: HeadersInit
  onResponse?: (response: Response) => void | Promise<void>
}

type RuntimeRequestOptions = Omit<RequestInit, 'method' | 'body' | 'headers'> & {
  parameters?: {
    path?: unknown
    query?: unknown
    header?: unknown
    cookie?: unknown
  }
  body?: unknown
  headers?: HeadersInit
}

/**
 * Contract-driven client. Exact path templates, parameter objects, request
 * bodies and successful response bodies are derived from the generated
 * production OpenAPI `paths` type.
 */
export function createContractClient(options: ApiClientOptions): ContractClient {
  async function execute(
    method: ContractHttpMethod,
    pathTemplate: string,
    requestOptions: RuntimeRequestOptions = {},
  ): Promise<Response> {
    const { parameters, body, headers: suppliedHeaders, ...init } = requestOptions
    const path = interpolatePath(pathTemplate, parameters?.path)
    const query = encodeQuery(parameters?.query)
    const baseUrl = options.baseUrl.endsWith('/')
      ? options.baseUrl.slice(0, -1)
      : options.baseUrl
    const headers = new Headers(options.headers)
    new Headers(suppliedHeaders).forEach((value, name) => headers.set(name, value))
    appendParameterHeaders(headers, parameters?.header)
    if (!headers.has('Accept')) headers.set('Accept', 'application/json')
    const csrfToken = options.getCsrfToken?.()
    if (csrfToken && !['get', 'head', 'options'].includes(method)) {
      headers.set('X-CSRF-Token', csrfToken)
    }

    let encodedBody: BodyInit | undefined
    if (body !== undefined) {
      if (isBodyInit(body)) {
        encodedBody = body
      } else {
        headers.set('Content-Type', 'application/json')
        encodedBody = JSON.stringify(body)
      }
    }

    const response = await (options.fetchImpl ?? globalThis.fetch)(`${baseUrl}${path}${query}`, {
      ...init,
      method: method.toUpperCase(),
      credentials: init.credentials ?? 'include',
      headers,
      ...(encodedBody === undefined ? {} : { body: encodedBody }),
    })
    await options.onResponse?.(response)
    if (!response.ok) throw await apiError(response)
    return response
  }

  async function request(
    method: ContractHttpMethod,
    path: string,
    requestOptions?: RuntimeRequestOptions,
  ): Promise<ContractResponse<unknown>> {
    const response = await execute(method, path, requestOptions)
    return {
      data: await responseBody(response),
      response,
      etag: response.headers.get('ETag'),
    }
  }

  return {
    request: request as unknown as ContractClient['request'],
    raw: execute as ContractClient['raw'],
    get: ((path: string, requestOptions?: RuntimeRequestOptions) =>
      request('get', path, requestOptions)) as unknown as ContractClient['get'],
    post: ((path: string, requestOptions?: RuntimeRequestOptions) =>
      request('post', path, requestOptions)) as unknown as ContractClient['post'],
    put: ((path: string, requestOptions?: RuntimeRequestOptions) =>
      request('put', path, requestOptions)) as unknown as ContractClient['put'],
    patch: ((path: string, requestOptions?: RuntimeRequestOptions) =>
      request('patch', path, requestOptions)) as unknown as ContractClient['patch'],
    delete: ((path: string, requestOptions?: RuntimeRequestOptions) =>
      request('delete', path, requestOptions)) as unknown as ContractClient['delete'],
  }
}

/** Backward-compatible untyped transport for incremental application migration. */
export function createApiClient(options: ApiClientOptions) {
  const fetchImpl = options.fetchImpl ?? fetch

  async function request<Data>(path: string, init: RequestInit = {}): Promise<Data> {
    const csrfToken = options.getCsrfToken?.()
    const headers = new Headers(options.headers)
    new Headers(init.headers).forEach((value, name) => headers.set(name, value))
    headers.set('Accept', 'application/json')
    if (init.body && !headers.has('Content-Type')) headers.set('Content-Type', 'application/json')
    if (csrfToken) headers.set('X-CSRF-Token', csrfToken)

    const response = await fetchImpl(`${options.baseUrl}${path}`, {
      ...init,
      credentials: 'include',
      headers,
    })
    if (!response.ok) throw await apiError(response)
    return await responseBody(response) as Data
  }

  return {
    get: <Data>(path: string, init?: RequestInit) => request<Data>(path, init),
    post: <Data>(path: string, body: unknown, init?: RequestInit) => request<Data>(path, {
      ...init,
      method: 'POST',
      body: JSON.stringify(body),
    }),
    patch: <Data>(path: string, body: unknown, etag: string, init?: RequestInit) => request<Data>(
      path,
      {
        ...init,
        method: 'PATCH',
        body: JSON.stringify(body),
        headers: { ...Object.fromEntries(new Headers(init?.headers)), 'If-Match': etag },
      },
    ),
  }
}

function interpolatePath(template: string, parameters: unknown): string {
  const values = asRecord(parameters)
  return template.replaceAll(/\{([^}]+)\}/g, (_, name: string) => {
    const value = values[name]
    if (value === undefined || value === null || value === '') {
      throw new TypeError(`Missing path parameter: ${name}`)
    }
    return encodeURIComponent(String(value))
  })
}

function encodeQuery(parameters: unknown): string {
  const values = asRecord(parameters)
  const query = new URLSearchParams()
  for (const [name, value] of Object.entries(values)) {
    if (value === undefined || value === null) continue
    if (Array.isArray(value)) {
      for (const item of value) query.append(name, String(item))
    } else {
      query.set(name, String(value))
    }
  }
  const encoded = query.toString()
  return encoded ? `?${encoded}` : ''
}

function appendParameterHeaders(headers: Headers, parameters: unknown): void {
  for (const [name, value] of Object.entries(asRecord(parameters))) {
    if (value !== undefined && value !== null) headers.set(name, String(value))
  }
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' ? value as Record<string, unknown> : {}
}

function isBodyInit(value: unknown): value is BodyInit {
  return value instanceof Blob
    || value instanceof FormData
    || value instanceof URLSearchParams
    || value instanceof ArrayBuffer
    || ArrayBuffer.isView(value)
    || value instanceof ReadableStream
}

async function responseBody(response: Response): Promise<unknown> {
  if (response.status === 204 || response.status === 205) return undefined
  const contentType = response.headers.get('Content-Type')?.toLowerCase() || ''
  if (contentType.includes('json')) return await response.json()
  return await response.text()
}

async function apiError(response: Response): Promise<ApiError> {
  const fallback: ProblemDetails = {
    type: 'about:blank',
    title: response.statusText || 'Request failed',
    status: response.status,
    detail: response.statusText || 'Request failed',
    requestId: response.headers.get('X-Request-ID') || '00000000-0000-0000-0000-000000000000',
  }
  const problem = await response.clone().json().catch(() => fallback) as ProblemDetails
  return new ApiError(problem, response)
}
