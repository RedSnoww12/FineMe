const TOKEN_KEY = 'fineme.token'
const BASE_URL = (import.meta.env.VITE_API_URL ?? '').replace(/\/+$/, '')

/** Adresse de l'API (`VITE_API_URL`) ; vide quand le front la sert sur la même origine (proxy). */
export function apiBaseUrl(): string {
  return BASE_URL
}

export const UNAUTHORIZED_EVENT = 'fineme:unauthorized'

export const UNREACHABLE_MESSAGE =
  'API injoignable. Vérifiez que le serveur FineMe est démarré et que vous êtes connecté à Internet, puis réessayez.'

export class ApiError extends Error {
  status: number
  /** Secondes à attendre avant de réessayer (en-tête `Retry-After` d'une réponse 429), sinon null. */
  retryAfter: number | null
  /** true quand le serveur n'a pas répondu (réseau, proxy, serveur arrêté). */
  unreachable: boolean

  constructor(message: string, status: number, unreachable = false, retryAfter: number | null = null) {
    super(message)
    this.name = 'ApiError'
    this.status = status
    this.unreachable = unreachable
    this.retryAfter = retryAfter
  }
}

export function getToken(): string | null {
  try {
    return localStorage.getItem(TOKEN_KEY)
  } catch {
    return null
  }
}

export function setToken(token: string | null) {
  try {
    if (token) localStorage.setItem(TOKEN_KEY, token)
    else localStorage.removeItem(TOKEN_KEY)
  } catch {
    // stockage indisponible (navigation privée) : la session ne survivra pas au rechargement
  }
}

/** `Retry-After` en secondes (entier positif) ; null si absent ou illisible. */
export function parseRetryAfter(value: string | null): number | null {
  if (!value || !/^\d+$/.test(value.trim())) return null
  const seconds = Number(value.trim())
  return seconds > 0 ? seconds : null
}

/** Attente imposée par l'API après trop de tentatives, si `error` en est une. */
export function retryAfterOf(error: unknown): number | null {
  return error instanceof ApiError ? error.retryAfter : null
}

export type Query =Record<string, string | number | null | undefined>

type RequestOptions = {
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE'
  body?: unknown
  query?: Query
}

function buildUrl(path: string, query?: Query) {
  const params = new URLSearchParams()
  for (const [key, value] of Object.entries(query ?? {})) {
    if (value !== null && value !== undefined && value !== '') params.set(key, String(value))
  }
  const qs = params.toString()
  return `${BASE_URL}${path}${qs ? `?${qs}` : ''}`
}

export async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const { method = 'GET', body, query } = options
  const headers: Record<string, string> = { Accept: 'application/json' }
  const token = getToken()
  if (token) headers.Authorization = `Bearer ${token}`
  if (body !== undefined) headers['Content-Type'] = 'application/json'

  let response: Response
  try {
    response = await fetch(buildUrl(path, query), {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    })
  } catch {
    throw new ApiError(UNREACHABLE_MESSAGE, 0, true)
  }

  if (response.status === 204) return undefined as T

  const text = await response.text()
  let data: unknown = undefined
  if (text) {
    try {
      data = JSON.parse(text)
    } catch {
      data = undefined
    }
  }

  if (!response.ok) {
    const apiMessage =
      data && typeof data === 'object' && 'error' in data && typeof data.error === 'string' ? data.error : null

    if (response.status === 401) {
      setToken(null)
      window.dispatchEvent(new Event(UNAUTHORIZED_EVENT))
      throw new ApiError(apiMessage ?? 'Session expirée. Reconnectez-vous.', 401)
    }
    // Une erreur serveur sans message JSON vient du proxy ou de l'hébergeur, pas de l'API.
    if (!apiMessage && response.status >= 500) {
      throw new ApiError(UNREACHABLE_MESSAGE, response.status, true)
    }
    throw new ApiError(
      apiMessage ?? `La requête a échoué (erreur ${response.status}).`,
      response.status,
      false,
      response.status === 429 ? parseRetryAfter(response.headers.get('Retry-After')) : null,
    )
  }

  if (data === undefined) {
    throw new ApiError(UNREACHABLE_MESSAGE, response.status, true)
  }
  return data as T
}

export function errorMessage(error: unknown): string {
  if (error instanceof ApiError) return error.message
  if (error instanceof Error && error.message) return error.message
  return 'Une erreur inattendue est survenue.'
}
