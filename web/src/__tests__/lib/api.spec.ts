import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

const mockAuthStore = {
  token: 'test-token',
  refresh: vi.fn(),
  logout: vi.fn(),
}

vi.mock('@/stores/auth', () => ({
  useAuthStore: () => mockAuthStore,
}))

import { get, post, put, del, setOnAuthFailure, adminApi } from '@/lib/api'

const mockOnAuthFailure = vi.fn()
setOnAuthFailure(mockOnAuthFailure)

function makeFetchResponse(status: number, body: unknown = {}) {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: () => Promise.resolve(body),
    text: () => Promise.resolve(JSON.stringify(body)),
  }
}

describe('api', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.restoreAllMocks()
    mockAuthStore.token = 'test-token'
    mockAuthStore.refresh = vi.fn()
    mockAuthStore.logout = vi.fn()
    mockOnAuthFailure.mockClear()
  })

  it('get adds Authorization header when token exists', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200, { data: 'ok' }))
    vi.stubGlobal('fetch', fetchMock)

    await get('/api/test')

    expect(fetchMock).toHaveBeenCalledTimes(1)
    const [url, options] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/test')
    expect(options.headers.get('Authorization')).toBe('Bearer test-token')
  })

  it('post sends JSON body', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await post('/api/items', { name: 'test' })

    const [, options] = fetchMock.mock.calls[0]
    expect(options.method).toBe('POST')
    expect(options.headers.get('Content-Type')).toBe('application/json')
    expect(options.body).toBe(JSON.stringify({ name: 'test' }))
  })

  it('del uses DELETE method', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await del('/api/items/1')

    const [, options] = fetchMock.mock.calls[0]
    expect(options.method).toBe('DELETE')
  })

  it('401 handling: attempts refresh, retries request on success', async () => {
    mockAuthStore.refresh.mockResolvedValue(true)
    mockAuthStore.token = 'refreshed-token'

    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(makeFetchResponse(401))
      .mockResolvedValueOnce(makeFetchResponse(200, { data: 'ok' }))
    vi.stubGlobal('fetch', fetchMock)

    await get('/api/protected')

    expect(mockAuthStore.refresh).toHaveBeenCalled()
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })

  it('401 + failed refresh: calls logout', async () => {
    mockAuthStore.refresh.mockResolvedValue(false)

    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(401))
    vi.stubGlobal('fetch', fetchMock)

    await get('/api/protected')

    expect(mockAuthStore.refresh).toHaveBeenCalled()
    expect(mockAuthStore.logout).toHaveBeenCalled()
    expect(mockOnAuthFailure).toHaveBeenCalled()
  })

  it('non-401 error passes through without refresh', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(500, { error: 'server error' }))
    vi.stubGlobal('fetch', fetchMock)

    const res = await get('/api/broken')

    expect(res.status).toBe(500)
    expect(mockAuthStore.refresh).not.toHaveBeenCalled()
    expect(mockAuthStore.logout).not.toHaveBeenCalled()
  })

  it('includes credentials in fetch calls', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await get('/api/test')

    const [, options] = fetchMock.mock.calls[0]
    expect(options.credentials).toBe('include')
  })

  it('does not set Authorization header when token is null', async () => {
    mockAuthStore.token = null as unknown as string
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await get('/api/public')

    const [, options] = fetchMock.mock.calls[0]
    expect(options.headers.has('Authorization')).toBe(false)
  })

  it('network error propagates', async () => {
    const fetchMock = vi.fn().mockRejectedValue(new TypeError('Failed to fetch'))
    vi.stubGlobal('fetch', fetchMock)

    await expect(get('/api/test')).rejects.toThrow('Failed to fetch')
  })

  it('401 without token does not attempt refresh', async () => {
    mockAuthStore.token = null as unknown as string
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(401))
    vi.stubGlobal('fetch', fetchMock)

    const res = await get('/api/protected')

    expect(res.status).toBe(401)
    expect(mockAuthStore.refresh).not.toHaveBeenCalled()
  })

  it('put sends JSON body with PUT method', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await put('/api/settings', { image_source: 'f' })

    const [, options] = fetchMock.mock.calls[0]
    expect(options.method).toBe('PUT')
    expect(options.headers.get('Content-Type')).toBe('application/json')
    expect(options.body).toBe(JSON.stringify({ image_source: 'f' }))
  })

  it('post without body does not set Content-Type', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await post('/api/action')

    const [, options] = fetchMock.mock.calls[0]
    expect(options.headers.has('Content-Type')).toBe(false)
    expect(options.body).toBeUndefined()
  })

  it('adminApi.fetchPoster calls POST with correct URL', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.fetchPoster('imdb', 'tt0111161')

    const [url, options] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/admin/posters/imdb/tt0111161/fetch')
    expect(options.method).toBe('POST')
  })

  it('adminApi.fetchPoster works with tmdb id type', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.fetchPoster('tmdb', '550')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/admin/posters/tmdb/550/fetch')
  })

  it('adminApi.previewPoster calls GET with correct URL and params', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.previewPoster(3, 'imdb,rt,tmdb')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('/api/admin/preview/poster')
    expect(url).toContain('ratings_limit=3')
    expect(url).toContain('ratings_order=imdb%2Crt%2Ctmdb')
  })

  it('adminApi.previewPoster includes position when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.previewPoster(3, 'imdb,rt', 'l')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('position=l')
  })

  it('adminApi.previewPoster omits position when not provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.previewPoster(3, 'imdb,rt')

    const [url] = fetchMock.mock.calls[0]
    expect(url).not.toContain('position')
  })

  it('adminApi.previewPoster includes label_style when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.previewPoster(3, 'imdb,rt', 'bc', 'h', 'i')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('label_style=i')
  })

  it('adminApi.previewPoster includes badge_direction when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.previewPoster(3, 'imdb,rt', 'bc', 'h', 'i', 'v')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('badge_direction=v')
  })

  it('adminApi.previewPoster omits badge_direction when not provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.previewPoster(3, 'imdb,rt')

    const [url] = fetchMock.mock.calls[0]
    expect(url).not.toContain('badge_direction')
  })

  it('adminApi.previewLogo includes label_style when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.previewLogo(3, 'imdb,rt', 'h', 'i')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('label_style=i')
  })

  it('adminApi.previewBackdrop includes label_style when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.previewBackdrop(3, 'imdb,rt', 'v', 'i')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('label_style=i')
  })

  it('adminApi.getLogos calls GET with correct URL', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.getLogos(1, 50)

    const [url] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/admin/logos?page=1&page_size=50')
  })

  it('adminApi.getLogoImage calls GET with correct URL', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.getLogoImage('imdb/tt0111161')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/admin/logos/imdb/tt0111161')
  })

  it('adminApi.fetchLogo calls POST with correct URL', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.fetchLogo('imdb', 'tt0111161')

    const [url, options] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/admin/logos/imdb/tt0111161/fetch')
    expect(options.method).toBe('POST')
  })

  it('adminApi.getBackdrops calls GET with correct URL', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.getBackdrops(1, 50)

    const [url] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/admin/backdrops?page=1&page_size=50')
  })

  it('adminApi.getBackdropImage calls GET with correct URL', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.getBackdropImage('tmdb/550')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/admin/backdrops/tmdb/550')
  })

  it('adminApi.fetchBackdrop calls POST with correct URL', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.fetchBackdrop('tmdb', '550')

    const [url, options] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/admin/backdrops/tmdb/550/fetch')
    expect(options.method).toBe('POST')
  })

  it('adminApi.getEpisodes calls GET with correct URL', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.getEpisodes(1, 50)

    const [url] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/admin/episodes?page=1&page_size=50')
  })

  it('adminApi.getEpisodeImage calls GET with correct URL', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.getEpisodeImage('tmdb/episode-1396-S1E1')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/admin/episodes/tmdb/episode-1396-S1E1/image')
  })

  it('adminApi.fetchEpisode calls POST with correct URL', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.fetchEpisode('tmdb', 'episode-1396-S1E1')

    const [url, options] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/admin/episodes/tmdb/episode-1396-S1E1/fetch')
    expect(options.method).toBe('POST')
  })

  it('adminApi.previewEpisode calls GET with correct URL and params', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.previewEpisode(3, 'imdb,rt,tmdb')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('/api/admin/preview/episode')
    expect(url).toContain('ratings_limit=3')
    expect(url).toContain('ratings_order=imdb%2Crt%2Ctmdb')
  })

  it('adminApi.previewEpisode includes position when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.previewEpisode(3, 'imdb,rt', undefined, undefined, undefined, 'tr')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('position=tr')
  })

  it('adminApi.previewEpisode includes badge_direction when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.previewEpisode(3, 'imdb,rt', undefined, undefined, undefined, 'tr', 'h')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('badge_direction=h')
  })

  it('adminApi.previewEpisode includes blur=true when enabled', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.previewEpisode(3, 'imdb,rt', undefined, undefined, undefined, undefined, undefined, true)

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('blur=true')
  })

  it('adminApi.previewEpisode omits blur when false', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.previewEpisode(3, 'imdb,rt', undefined, undefined, undefined, undefined, undefined, false)

    const [url] = fetchMock.mock.calls[0]
    expect(url).not.toContain('blur')
  })

  it('adminApi.previewEpisode includes label_style when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.previewEpisode(3, 'imdb,rt', 'v', 'i')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('label_style=i')
  })

  it('adminApi.getSeasons calls GET with correct URL', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.getSeasons(1, 50)

    const [url] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/admin/seasons?page=1&page_size=50')
  })

  it('adminApi.getSeasonImage calls GET with correct URL', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.getSeasonImage('tmdb/872585-S1')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/admin/seasons/tmdb/872585-S1/image')
  })

  it('adminApi.fetchSeason calls POST with correct URL', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.fetchSeason('tmdb', '872585-S1')

    const [url, options] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/admin/seasons/tmdb/872585-S1/fetch')
    expect(options.method).toBe('POST')
  })

  it('adminApi.previewSeason calls GET with correct URL and params', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.previewSeason(3, 'imdb,rt,tmdb')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('/api/admin/preview/season')
    expect(url).toContain('ratings_limit=3')
    expect(url).toContain('ratings_order=imdb%2Crt%2Ctmdb')
  })

  it('adminApi.previewSeason includes position when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.previewSeason(3, 'imdb,rt', undefined, undefined, undefined, 'bc')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('position=bc')
  })

  it('adminApi.previewSeason includes badge_direction when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.previewSeason(3, 'imdb,rt', undefined, undefined, undefined, 'bc', 'h')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('badge_direction=h')
  })

  it('adminApi.previewSeason includes label_style when provided', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.previewSeason(3, 'imdb,rt', 'v', 'i')

    const [url] = fetchMock.mock.calls[0]
    expect(url).toContain('label_style=i')
  })

  it('adminApi.previewSeason has no blur parameter', async () => {
    const fetchMock = vi.fn().mockResolvedValue(makeFetchResponse(200))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.previewSeason(3, 'imdb,rt', 'v', 'i', 'm', 'bc', 'v')

    const [url] = fetchMock.mock.calls[0]
    expect(url).not.toContain('blur')
  })
})
