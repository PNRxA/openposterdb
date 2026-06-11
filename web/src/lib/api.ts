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

export interface SaveSettingsPayload {
  image_source: string
  lang: string
  textless: boolean
  ratings_limit: number
  ratings_order: string
  ratings_exclude: string
  poster_position: string
  logo_ratings_limit: number
  backdrop_ratings_limit: number
  poster_badge_style: string
  logo_badge_style: string
  backdrop_badge_style: string
  poster_label_style: string
  logo_label_style: string
  backdrop_label_style: string
  poster_badge_direction: string
  poster_badge_split: boolean
  poster_fit: string
  poster_badge_size: string
  logo_badge_size: string
  backdrop_badge_size: string
  backdrop_position: string
  backdrop_badge_direction: string
  backdrop_edge_inset_x: number
  backdrop_edge_inset_y: number
  episode_ratings_limit: number
  episode_badge_style: string
  episode_label_style: string
  episode_badge_size: string
  episode_position: string
  episode_badge_direction: string
  episode_blur: boolean
  season_ratings_limit: number
  season_badge_style: string
  season_label_style: string
  season_badge_size: string
  season_position: string
  season_badge_direction: string
  poster_badge_shape: string
  logo_badge_shape: string
  backdrop_badge_shape: string
  episode_badge_shape: string
  season_badge_shape: string
  poster_badge_background: string
  logo_badge_background: string
  backdrop_badge_background: string
  episode_badge_background: string
  season_badge_background: string
}

/** Build a URL path with query parameters, omitting entries with nullish values. */
function buildUrl(path: string, params: Record<string, string | number | undefined>): string {
  const qs = new URLSearchParams()
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined && value !== '') {
      qs.set(key, String(value))
    }
  }
  const query = qs.toString()
  return query ? `${path}?${query}` : path
}

// --- Typed API service layer ---

export const adminApi = {
  getStats: (): Promise<Response> => get('/api/admin/stats'),
  getPosters: (page: number, pageSize: number): Promise<Response> =>
    get(`/api/admin/posters?page=${page}&page_size=${pageSize}`),
  getPosterImage: (key: string): Promise<Response> =>
    get(`/api/admin/posters/${key}/image`),
  getSettings: (): Promise<Response> => get('/api/admin/settings'),
  updateSettings: (settings: Partial<SaveSettingsPayload> & { image_source: string; free_api_key_enabled?: boolean }): Promise<Response> => put('/api/admin/settings', settings),
  fetchPoster: (idType: string, idValue: string): Promise<Response> =>
    post(`/api/admin/posters/${idType}/${idValue}/fetch`),
  getLogos: (page: number, pageSize: number): Promise<Response> =>
    get(`/api/admin/logos?page=${page}&page_size=${pageSize}`),
  getLogoImage: (key: string): Promise<Response> =>
    get(`/api/admin/logos/${key}`),
  fetchLogo: (idType: string, idValue: string): Promise<Response> =>
    post(`/api/admin/logos/${idType}/${idValue}/fetch`),
  getBackdrops: (page: number, pageSize: number): Promise<Response> =>
    get(`/api/admin/backdrops?page=${page}&page_size=${pageSize}`),
  getBackdropImage: (key: string): Promise<Response> =>
    get(`/api/admin/backdrops/${key}`),
  fetchBackdrop: (idType: string, idValue: string): Promise<Response> =>
    post(`/api/admin/backdrops/${idType}/${idValue}/fetch`),
  getEpisodes: (page: number, pageSize: number): Promise<Response> =>
    get(`/api/admin/episodes?page=${page}&page_size=${pageSize}`),
  getEpisodeImage: (key: string): Promise<Response> =>
    get(`/api/admin/episodes/${key}/image`),
  fetchEpisode: (idType: string, idValue: string): Promise<Response> =>
    post(`/api/admin/episodes/${idType}/${idValue}/fetch`),
  getSeasons: (page: number, pageSize: number): Promise<Response> =>
    get(`/api/admin/seasons?page=${page}&page_size=${pageSize}`),
  getSeasonImage: (key: string): Promise<Response> =>
    get(`/api/admin/seasons/${key}/image`),
  fetchSeason: (idType: string, idValue: string): Promise<Response> =>
    post(`/api/admin/seasons/${idType}/${idValue}/fetch`),
  previewPoster: (ratingsLimit: number, ratingsOrder: string, posterPosition?: string, badgeStyle?: string, labelStyle?: string, badgeDirection?: string, badgeSize?: string, ratingsExclude?: string, posterSplit?: boolean, badgeShape?: string, badgeBackground?: string, posterFit?: string): Promise<Response> =>
    get(buildUrl('/api/admin/preview/poster', { ratings_limit: ratingsLimit, ratings_order: ratingsOrder, ratings_exclude: ratingsExclude, position: posterPosition, badge_style: badgeStyle, label_style: labelStyle, badge_direction: badgeDirection, badge_size: badgeSize, split: posterSplit ? 'true' : undefined, badge_shape: badgeShape, badge_background: badgeBackground, fit: posterFit })),
  previewLogo: (ratingsLimit: number, ratingsOrder: string, badgeStyle?: string, labelStyle?: string, badgeSize?: string, ratingsExclude?: string, badgeShape?: string, badgeBackground?: string): Promise<Response> =>
    get(buildUrl('/api/admin/preview/logo', { ratings_limit: ratingsLimit, ratings_order: ratingsOrder, ratings_exclude: ratingsExclude, badge_style: badgeStyle, label_style: labelStyle, badge_size: badgeSize, badge_shape: badgeShape, badge_background: badgeBackground })),
  previewBackdrop: (ratingsLimit: number, ratingsOrder: string, badgeStyle?: string, labelStyle?: string, badgeSize?: string, position?: string, badgeDirection?: string, ratingsExclude?: string, badgeShape?: string, badgeBackground?: string, edgeInsetX?: number, edgeInsetY?: number): Promise<Response> =>
    get(buildUrl('/api/admin/preview/backdrop', { ratings_limit: ratingsLimit, ratings_order: ratingsOrder, ratings_exclude: ratingsExclude, badge_style: badgeStyle, label_style: labelStyle, badge_size: badgeSize, position, badge_direction: badgeDirection, badge_shape: badgeShape, badge_background: badgeBackground, edge_inset_x: edgeInsetX, edge_inset_y: edgeInsetY })),
  previewEpisode: (ratingsLimit: number, ratingsOrder: string, badgeStyle?: string, labelStyle?: string, badgeSize?: string, position?: string, badgeDirection?: string, blur?: boolean, ratingsExclude?: string, badgeShape?: string, badgeBackground?: string): Promise<Response> =>
    get(buildUrl('/api/admin/preview/episode', { ratings_limit: ratingsLimit, ratings_order: ratingsOrder, ratings_exclude: ratingsExclude, badge_style: badgeStyle, label_style: labelStyle, badge_size: badgeSize, position, badge_direction: badgeDirection, blur: blur ? 'true' : undefined, badge_shape: badgeShape, badge_background: badgeBackground })),
  previewSeason: (ratingsLimit: number, ratingsOrder: string, badgeStyle?: string, labelStyle?: string, badgeSize?: string, position?: string, badgeDirection?: string, ratingsExclude?: string, badgeShape?: string, badgeBackground?: string): Promise<Response> =>
    get(buildUrl('/api/admin/preview/season', { ratings_limit: ratingsLimit, ratings_order: ratingsOrder, ratings_exclude: ratingsExclude, badge_style: badgeStyle, label_style: labelStyle, badge_size: badgeSize, position, badge_direction: badgeDirection, badge_shape: badgeShape, badge_background: badgeBackground })),
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
  updateSettings: (settings: SaveSettingsPayload): Promise<Response> =>
    keyRequest('/api/key/me/settings', {
      method: 'PUT',
      body: JSON.stringify(settings),
    }),
  resetSettings: (): Promise<Response> =>
    keyRequest('/api/key/me/settings', { method: 'DELETE' }),
  previewPoster: (ratingsLimit: number, ratingsOrder: string, posterPosition?: string, badgeStyle?: string, labelStyle?: string, badgeDirection?: string, badgeSize?: string, ratingsExclude?: string, posterSplit?: boolean, badgeShape?: string, badgeBackground?: string, posterFit?: string): Promise<Response> =>
    keyRequest(buildUrl('/api/key/me/preview/poster', { ratings_limit: ratingsLimit, ratings_order: ratingsOrder, ratings_exclude: ratingsExclude, position: posterPosition, badge_style: badgeStyle, label_style: labelStyle, badge_direction: badgeDirection, badge_size: badgeSize, split: posterSplit ? 'true' : undefined, badge_shape: badgeShape, badge_background: badgeBackground, fit: posterFit })),
  previewLogo: (ratingsLimit: number, ratingsOrder: string, badgeStyle?: string, labelStyle?: string, badgeSize?: string, ratingsExclude?: string, badgeShape?: string, badgeBackground?: string): Promise<Response> =>
    keyRequest(buildUrl('/api/key/me/preview/logo', { ratings_limit: ratingsLimit, ratings_order: ratingsOrder, ratings_exclude: ratingsExclude, badge_style: badgeStyle, label_style: labelStyle, badge_size: badgeSize, badge_shape: badgeShape, badge_background: badgeBackground })),
  previewBackdrop: (ratingsLimit: number, ratingsOrder: string, badgeStyle?: string, labelStyle?: string, badgeSize?: string, position?: string, badgeDirection?: string, ratingsExclude?: string, badgeShape?: string, badgeBackground?: string, edgeInsetX?: number, edgeInsetY?: number): Promise<Response> =>
    keyRequest(buildUrl('/api/key/me/preview/backdrop', { ratings_limit: ratingsLimit, ratings_order: ratingsOrder, ratings_exclude: ratingsExclude, badge_style: badgeStyle, label_style: labelStyle, badge_size: badgeSize, position, badge_direction: badgeDirection, badge_shape: badgeShape, badge_background: badgeBackground, edge_inset_x: edgeInsetX, edge_inset_y: edgeInsetY })),
  previewEpisode: (ratingsLimit: number, ratingsOrder: string, badgeStyle?: string, labelStyle?: string, badgeSize?: string, position?: string, badgeDirection?: string, blur?: boolean, ratingsExclude?: string, badgeShape?: string, badgeBackground?: string): Promise<Response> =>
    keyRequest(buildUrl('/api/key/me/preview/episode', { ratings_limit: ratingsLimit, ratings_order: ratingsOrder, ratings_exclude: ratingsExclude, badge_style: badgeStyle, label_style: labelStyle, badge_size: badgeSize, position, badge_direction: badgeDirection, blur: blur ? 'true' : undefined, badge_shape: badgeShape, badge_background: badgeBackground })),
  previewSeason: (ratingsLimit: number, ratingsOrder: string, badgeStyle?: string, labelStyle?: string, badgeSize?: string, position?: string, badgeDirection?: string, ratingsExclude?: string, badgeShape?: string, badgeBackground?: string): Promise<Response> =>
    keyRequest(buildUrl('/api/key/me/preview/season', { ratings_limit: ratingsLimit, ratings_order: ratingsOrder, ratings_exclude: ratingsExclude, badge_style: badgeStyle, label_style: labelStyle, badge_size: badgeSize, position, badge_direction: badgeDirection, badge_shape: badgeShape, badge_background: badgeBackground })),
}

export const keysApi = {
  list: (): Promise<Response> => get('/api/keys'),
  create: (name: string): Promise<Response> => post('/api/keys', { name }),
  delete: (id: number): Promise<Response> => del(`/api/keys/${id}`),
  getSettings: (id: number): Promise<Response> => get(`/api/keys/${id}/settings`),
  updateSettings: (id: number, settings: SaveSettingsPayload): Promise<Response> => put(`/api/keys/${id}/settings`, settings),
  deleteSettings: (id: number): Promise<Response> => del(`/api/keys/${id}/settings`),
}
