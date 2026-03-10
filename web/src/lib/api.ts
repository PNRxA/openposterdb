import { useAuthStore } from '@/stores/auth'

const BASE_URL = import.meta.env.VITE_API_URL || ''

let _onAuthFailure: (() => void) | null = null

export function setOnAuthFailure(callback: () => void) {
  _onAuthFailure = callback
}

async function request(path: string, options: RequestInit = {}): Promise<Response> {
  const auth = useAuthStore()
  const headers = new Headers(options.headers)

  if (auth.token) {
    headers.set('Authorization', `Bearer ${auth.token}`)
  }

  if (options.body && !headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json')
  }

  const res = await fetch(`${BASE_URL}${path}`, { ...options, headers, credentials: 'include' })

  if (res.status === 401 && auth.token) {
    // Try refreshing the token
    const refreshed = await auth.refresh()
    if (refreshed) {
      const retryHeaders = new Headers(options.headers)
      retryHeaders.set('Authorization', `Bearer ${auth.token}`)
      if (options.body && !retryHeaders.has('Content-Type')) {
        retryHeaders.set('Content-Type', 'application/json')
      }
      return fetch(`${BASE_URL}${path}`, { ...options, headers: retryHeaders, credentials: 'include' })
    }
    // Refresh failed — clear credentials and redirect without retrying
    auth.logout()
    _onAuthFailure?.()
    return res
  }

  return res
}

export async function get(path: string): Promise<Response> {
  return request(path)
}

export async function post(path: string, body?: unknown): Promise<Response> {
  return request(path, {
    method: 'POST',
    body: body ? JSON.stringify(body) : undefined,
  })
}

export async function put(path: string, body?: unknown): Promise<Response> {
  return request(path, {
    method: 'PUT',
    body: body ? JSON.stringify(body) : undefined,
  })
}

export async function del(path: string): Promise<Response> {
  return request(path, { method: 'DELETE' })
}

// --- Typed API service layer ---

export const adminApi = {
  getStats: (): Promise<Response> => get('/api/admin/stats'),
  getPosters: (page: number, pageSize: number): Promise<Response> =>
    get(`/api/admin/posters?page=${page}&page_size=${pageSize}`),
  getPosterImage: (key: string): Promise<Response> =>
    get(`/api/admin/posters/${key}/image`),
  getSettings: (): Promise<Response> => get('/api/admin/settings'),
  updateSettings: (settings: {
    poster_source: string
    fanart_lang: string
    fanart_textless: boolean
  }): Promise<Response> => put('/api/admin/settings', settings),
}

// --- Self-service API (API key session JWT auth) ---

const KEY_BASE_URL = import.meta.env.VITE_API_URL || ''

function keyRequest(path: string, options: RequestInit = {}): Promise<Response> {
  const auth = useAuthStore()
  const headers = new Headers(options.headers)

  if (auth.apiKeyToken) {
    headers.set('Authorization', `Bearer ${auth.apiKeyToken}`)
  }

  if (options.body && !headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json')
  }

  return fetch(`${KEY_BASE_URL}${path}`, { ...options, headers })
}

export const selfApi = {
  getInfo: (): Promise<Response> => keyRequest('/api/key/me'),
  getSettings: (): Promise<Response> => keyRequest('/api/key/me/settings'),
  updateSettings: (settings: {
    poster_source: string
    fanart_lang: string
    fanart_textless: boolean
  }): Promise<Response> =>
    keyRequest('/api/key/me/settings', {
      method: 'PUT',
      body: JSON.stringify(settings),
    }),
  resetSettings: (): Promise<Response> =>
    keyRequest('/api/key/me/settings', { method: 'DELETE' }),
}

export const keysApi = {
  list: (): Promise<Response> => get('/api/keys'),
  create: (name: string): Promise<Response> => post('/api/keys', { name }),
  delete: (id: number): Promise<Response> => del(`/api/keys/${id}`),
  getSettings: (id: number): Promise<Response> => get(`/api/keys/${id}/settings`),
  updateSettings: (
    id: number,
    settings: {
      poster_source: string
      fanart_lang: string
      fanart_textless: boolean
    },
  ): Promise<Response> => put(`/api/keys/${id}/settings`, settings),
  deleteSettings: (id: number): Promise<Response> => del(`/api/keys/${id}/settings`),
}
