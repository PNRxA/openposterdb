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
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200, { poster_source: 't' }))
    vi.stubGlobal('fetch', fetchMock)

    await selfApi.getSettings()

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('/api/key/me/settings')
  })

  it('updateSettings sends PUT with JSON body', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200, { ok: true }))
    vi.stubGlobal('fetch', fetchMock)

    await selfApi.updateSettings({
      poster_source: 'f',
      fanart_lang: 'de',
      fanart_textless: true,
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
    })

    const [url, options] = fetchMock.mock.calls[0]
    expect(url).toContain('/api/key/me/settings')
    expect(options.method).toBe('PUT')
    expect(options.headers.get('Content-Type')).toBe('application/json')
    expect(JSON.parse(options.body)).toEqual({
      poster_source: 'f',
      fanart_lang: 'de',
      fanart_textless: true,
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

  it('previewPoster includes poster_position when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await selfApi.previewPoster(3, 'imdb,rt', 'r')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('poster_position=r')
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
})
