import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

const mockAuthStore = {
  token: null as string | null,
  apiKeyToken: 'test-jwt-token',
  refresh: vi.fn(),
  logout: vi.fn(),
}

vi.mock('@/stores/auth', () => ({
  useAuthStore: () => mockAuthStore,
}))

import { selfApi } from '@/lib/api'

function makeFetchResponse(status: number, body: unknown = {}) {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: () => Promise.resolve(body),
  }
}

describe('selfApi', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.restoreAllMocks()
    mockAuthStore.apiKeyToken = 'test-jwt-token'
    mockAuthStore.token = null
  })

  it('getInfo sends GET with JWT token as Bearer token', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200, { name: 'k', key_prefix: 'ab' }))
    vi.stubGlobal('fetch', fetchMock)

    await selfApi.getInfo()

    expect(fetchMock).toHaveBeenCalledTimes(1)
    const [url, options] = fetchMock.mock.calls[0]
    expect(url).toContain('/api/key/me')
    expect(options.headers.get('Authorization')).toBe('Bearer test-jwt-token')
  })

  it('getSettings sends GET to /api/key/me/settings', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200, { image_source: 't' }))
    vi.stubGlobal('fetch', fetchMock)

    await selfApi.getSettings()

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('/api/key/me/settings')
  })

  it('updateSettings sends PUT with JSON body', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200, { ok: true }))
    vi.stubGlobal('fetch', fetchMock)

    await selfApi.updateSettings({
      image_source: 'f',
      lang: 'de',
      textless: true,
      ratings_limit: 3,
      ratings_order: 'mal,imdb,trakt',
      poster_position: 'bc',
      logo_ratings_limit: 3,
      backdrop_ratings_limit: 3,
      poster_badge_style: 'h',
      logo_badge_style: 'h',
      backdrop_badge_style: 'v',
      poster_label_style: 't',
      logo_label_style: 't',
      backdrop_label_style: 't',
      poster_badge_direction: 'd',
      poster_badge_size: 'l',
      poster_badge_scale_override: 1,
      logo_badge_size: 's',
      logo_badge_scale_override: 1,
      backdrop_badge_size: 'xl',
      backdrop_badge_scale_override: 1,
      backdrop_position: 'tr',
      backdrop_badge_direction: 'v',
      episode_ratings_limit: 1,
      episode_badge_style: 'v',
      episode_label_style: 'o',
      episode_badge_size: 'l',
      episode_badge_scale_override: 1,
      episode_position: 'tr',
      episode_badge_direction: 'v',
      episode_blur: false,
      poster_badge_gap: 0,
      poster_badge_margin: 0,
      logo_badge_gap: 0,
      logo_badge_margin: 0,
      backdrop_badge_gap: 0,
      backdrop_badge_margin: 0,
      episode_badge_gap: 0,
      episode_badge_margin: 0,
    })

    const [url, options] = fetchMock.mock.calls[0]
    expect(url).toContain('/api/key/me/settings')
    expect(options.method).toBe('PUT')
    expect(options.headers.get('Content-Type')).toBe('application/json')
    expect(JSON.parse(options.body)).toEqual({
      image_source: 'f',
      lang: 'de',
      textless: true,
      ratings_limit: 3,
      ratings_order: 'mal,imdb,trakt',
      poster_position: 'bc',
      logo_ratings_limit: 3,
      backdrop_ratings_limit: 3,
      poster_badge_style: 'h',
      logo_badge_style: 'h',
      backdrop_badge_style: 'v',
      poster_label_style: 't',
      logo_label_style: 't',
      backdrop_label_style: 't',
      poster_badge_direction: 'd',
      poster_badge_size: 'l',
      poster_badge_scale_override: 1,
      logo_badge_size: 's',
      logo_badge_scale_override: 1,
      backdrop_badge_size: 'xl',
      backdrop_badge_scale_override: 1,
      backdrop_position: 'tr',
      backdrop_badge_direction: 'v',
      episode_ratings_limit: 1,
      episode_badge_style: 'v',
      episode_label_style: 'o',
      episode_badge_size: 'l',
      episode_badge_scale_override: 1,
      episode_position: 'tr',
      episode_badge_direction: 'v',
      episode_blur: false,
      poster_badge_gap: 0,
      poster_badge_margin: 0,
      logo_badge_gap: 0,
      logo_badge_margin: 0,
      backdrop_badge_gap: 0,
      backdrop_badge_margin: 0,
      episode_badge_gap: 0,
      episode_badge_margin: 0,
    })
  })

  it('resetSettings sends DELETE', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200, { ok: true }))
    vi.stubGlobal('fetch', fetchMock)

    await selfApi.resetSettings()

    const [url, options] = fetchMock.mock.calls[0]
    expect(url).toContain('/api/key/me/settings')
    expect(options.method).toBe('DELETE')
  })

  it('does not set Authorization when apiKeyToken is null', async () => {
    mockAuthStore.apiKeyToken = null as unknown as string
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await selfApi.getInfo()

    const [, options] = fetchMock.mock.calls[0]
    expect(options.headers.has('Authorization')).toBe(false)
  })

  it('does not include credentials (no cookie needed)', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await selfApi.getInfo()

    const [, options] = fetchMock.mock.calls[0]
    // keyRequest does not set credentials: 'include'
    expect(options.credentials).toBeUndefined()
  })

  it('previewPoster includes position when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await selfApi.previewPoster(3, 'imdb,rt', 'r')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('position=r')
  })

  it('previewPoster includes label_style when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await selfApi.previewPoster(3, 'imdb,rt', 'bc', 'h', 'i')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('label_style=i')
  })

  it('previewPoster includes badge_direction when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await selfApi.previewPoster(3, 'imdb,rt', 'bc', 'h', 'i', 'v')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('badge_direction=v')
  })

  it('previewLogo includes label_style when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await selfApi.previewLogo(3, 'imdb,rt', 'h', 'i')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('label_style=i')
  })

  it('previewBackdrop includes label_style when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await selfApi.previewBackdrop(3, 'imdb,rt', 'v', 'i')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('label_style=i')
  })

  it('previewEpisode calls GET with correct URL and params', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await selfApi.previewEpisode(3, 'imdb,rt,tmdb')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('/api/key/me/preview/episode')
    expect(url).toContain('ratings_limit=3')
    expect(url).toContain('ratings_order=imdb%2Crt%2Ctmdb')
  })

  it('previewEpisode includes position when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await selfApi.previewEpisode(3, 'imdb,rt', undefined, undefined, undefined, 'tr')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('position=tr')
  })

  it('previewEpisode includes badge_direction when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await selfApi.previewEpisode(3, 'imdb,rt', undefined, undefined, undefined, 'tr', 'h')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('badge_direction=h')
  })

  it('previewEpisode includes blur=true when enabled', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await selfApi.previewEpisode(3, 'imdb,rt', undefined, undefined, undefined, undefined, undefined, true, undefined)

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('blur=true')
  })

  it('previewEpisode omits blur when false', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await selfApi.previewEpisode(3, 'imdb,rt', undefined, undefined, undefined, undefined, undefined, false, undefined)

    const [url] = fetchMock.mock.calls[0]
    expect(url).not.toContain('blur')
  })

  it('previewEpisode includes label_style when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await selfApi.previewEpisode(3, 'imdb,rt', 'v', 'i')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('label_style=i')
  })

  it('updateSettings includes episode fields', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200, { ok: true }))
    vi.stubGlobal('fetch', fetchMock)

    await selfApi.updateSettings({
      image_source: 't',
      lang: 'en',
      textless: false,
      ratings_limit: 3,
      ratings_order: 'imdb,rt,tmdb',
      poster_position: 'bc',
      logo_ratings_limit: 3,
      backdrop_ratings_limit: 3,
      poster_badge_style: 'h',
      logo_badge_style: 'h',
      backdrop_badge_style: 'v',
      poster_label_style: 't',
      logo_label_style: 't',
      backdrop_label_style: 't',
      poster_badge_direction: 'd',
      poster_badge_size: 'l',
      poster_badge_scale_override: 1,
      logo_badge_size: 's',
      logo_badge_scale_override: 1,
      backdrop_badge_size: 'xl',
      backdrop_badge_scale_override: 1,
      backdrop_position: 'tr',
      backdrop_badge_direction: 'v',
      episode_ratings_limit: 1,
      episode_badge_style: 'v',
      episode_label_style: 'o',
      episode_badge_size: 'l',
      episode_badge_scale_override: 1,
      episode_position: 'tr',
      episode_badge_direction: 'v',
      episode_blur: false,
      poster_badge_gap: 0,
      poster_badge_margin: 0,
      logo_badge_gap: 0,
      logo_badge_margin: 0,
      backdrop_badge_gap: 0,
      backdrop_badge_margin: 0,
      episode_badge_gap: 0,
      episode_badge_margin: 0,
    })

    const [, options] = fetchMock.mock.calls[0]
    const body = JSON.parse(options.body)
    expect(body.episode_ratings_limit).toBe(1)
    expect(body.episode_badge_style).toBe('v')
    expect(body.episode_label_style).toBe('o')
    expect(body.episode_badge_size).toBe('l')
    expect(body.episode_badge_scale_override).toBe(1)
    expect(body.episode_position).toBe('tr')
    expect(body.episode_badge_direction).toBe('v')
    expect(body.episode_blur).toBe(false)
  })
})
