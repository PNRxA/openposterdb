const BASE_URL = import.meta.env.VITE_API_URL || ''

/**
 * Global default render settings the free API key serves with, returned by
 * `GET /api/free-key/settings`. Enum-coded fields use the same short codes as
 * the image query params (e.g. badge style `h`/`v`/`d`, image source `t`/`f`).
 */
export interface FreeKeyDefaults {
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
  poster_badge_shape: string
  logo_badge_shape: string
  backdrop_badge_shape: string
  episode_badge_shape: string
  poster_badge_background: string
  logo_badge_background: string
  backdrop_badge_background: string
  episode_badge_background: string
  quality_style: string
  quality_direction: string
  poster_lang_icon: string
  logo_lang_icon: string
  backdrop_lang_icon: string
  lang_exclude: string
  poster_quality_position: string
  backdrop_quality_position: string
  poster_lang_position: string
  backdrop_lang_position: string
}

export const authApi = {
  status: (): Promise<Response> =>
    fetch(`${BASE_URL}/api/auth/status`),
  freeKeySettings: (): Promise<Response> =>
    fetch(`${BASE_URL}/api/free-key/settings`),
  setup: (username: string, password: string): Promise<Response> =>
    fetch(`${BASE_URL}/api/auth/setup`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username, password }),
      credentials: 'include',
    }),
  login: (username: string, password: string): Promise<Response> =>
    fetch(`${BASE_URL}/api/auth/login`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username, password }),
      credentials: 'include',
    }),
  refresh: (): Promise<Response> =>
    fetch(`${BASE_URL}/api/auth/refresh`, {
      method: 'POST',
      credentials: 'include',
    }),
  keyLogin: (apiKey: string): Promise<Response> =>
    fetch(`${BASE_URL}/api/auth/key-login`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ api_key: apiKey }),
    }),
}
