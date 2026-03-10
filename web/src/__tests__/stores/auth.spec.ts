import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useAuthStore } from '@/stores/auth'

const localStorageMock: Record<string, string> = {}
const localStorageStub = {
  getItem: vi.fn((key: string) => localStorageMock[key] ?? null),
  setItem: vi.fn((key: string, value: string) => {
    localStorageMock[key] = value
  }),
  removeItem: vi.fn((key: string) => {
    delete localStorageMock[key]
  }),
}

const sessionStorageMock: Record<string, string> = {}
const sessionStorageStub = {
  getItem: vi.fn((key: string) => sessionStorageMock[key] ?? null),
  setItem: vi.fn((key: string, value: string) => {
    sessionStorageMock[key] = value
  }),
  removeItem: vi.fn((key: string) => {
    delete sessionStorageMock[key]
  }),
}

vi.stubGlobal('localStorage', localStorageStub)
vi.stubGlobal('sessionStorage', sessionStorageStub)

function mockFetchSuccess(data: Record<string, unknown>, ok = true) {
  return vi.fn().mockResolvedValue({
    ok,
    status: ok ? 200 : 401,
    json: () => Promise.resolve(data),
  })
}

describe('auth store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    Object.keys(localStorageMock).forEach((k) => delete localStorageMock[k])
    Object.keys(sessionStorageMock).forEach((k) => delete sessionStorageMock[k])
    vi.restoreAllMocks()
    vi.stubGlobal('localStorage', localStorageStub)
    vi.stubGlobal('sessionStorage', sessionStorageStub)
  })

  it('token is set via login and stored in localStorage', async () => {
    vi.stubGlobal('fetch', mockFetchSuccess({ token: 'abc123' }))
    const auth = useAuthStore()
    await auth.login('user', 'pass')
    expect(auth.token).toBe('abc123')
    expect(localStorageStub.setItem).toHaveBeenCalledWith('token', 'abc123')
  })

  it('logout clears token from localStorage and sets null', async () => {
    vi.stubGlobal('fetch', mockFetchSuccess({ token: 'abc123' }))
    const auth = useAuthStore()
    await auth.login('user', 'pass')
    auth.logout()
    expect(auth.token).toBeNull()
    expect(localStorageStub.removeItem).toHaveBeenCalledWith('token')
  })

  it('isAuthenticated is true when token set', async () => {
    vi.stubGlobal('fetch', mockFetchSuccess({ token: 'abc123' }))
    const auth = useAuthStore()
    await auth.login('user', 'pass')
    expect(auth.isAuthenticated).toBe(true)
  })

  it('isAuthenticated is false when token is null', () => {
    const auth = useAuthStore()
    expect(auth.isAuthenticated).toBe(false)
  })

  it('login returns true on success', async () => {
    const fetchMock = mockFetchSuccess({ token: 'new-token' })
    vi.stubGlobal('fetch', fetchMock)
    const auth = useAuthStore()

    const result = await auth.login('user', 'pass')

    expect(result).toBe(true)
    expect(auth.token).toBe('new-token')
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining('/api/auth/login'),
      expect.objectContaining({ method: 'POST' }),
    )
  })

  it('login returns false on failure', async () => {
    const fetchMock = mockFetchSuccess({}, false)
    vi.stubGlobal('fetch', fetchMock)
    const auth = useAuthStore()

    const result = await auth.login('user', 'wrong')

    expect(result).toBe(false)
    expect(auth.token).toBeNull()
  })

  it('setup sets token and setupRequired=false', async () => {
    const fetchMock = mockFetchSuccess({ token: 'setup-token' })
    vi.stubGlobal('fetch', fetchMock)
    const auth = useAuthStore()

    const result = await auth.setup('admin', 'password123')

    expect(result).toBe(true)
    expect(auth.token).toBe('setup-token')
    expect(auth.setupRequired).toBe(false)
  })

  it('refresh updates token', async () => {
    const fetchMock = mockFetchSuccess({ token: 'refreshed-token' })
    vi.stubGlobal('fetch', fetchMock)
    const auth = useAuthStore()

    const result = await auth.refresh()

    expect(result).toBe(true)
    expect(auth.token).toBe('refreshed-token')
  })

  it('logout clears token and sets setupRequired to null', async () => {
    vi.stubGlobal('fetch', mockFetchSuccess({ token: 'abc' }))
    const auth = useAuthStore()
    await auth.login('user', 'pass')
    auth.logout()
    expect(auth.token).toBeNull()
    expect(auth.setupRequired).toBeNull()
  })

  it('checkSetupRequired caches result on second call', async () => {
    const fetchMock = mockFetchSuccess({ setup_required: true })
    vi.stubGlobal('fetch', fetchMock)
    const auth = useAuthStore()

    const first = await auth.checkSetupRequired()
    const second = await auth.checkSetupRequired()

    expect(first).toBe(true)
    expect(second).toBe(true)
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })

  it('refresh returns false on failure', async () => {
    const fetchMock = mockFetchSuccess({}, false)
    vi.stubGlobal('fetch', fetchMock)
    const auth = useAuthStore()

    const result = await auth.refresh()

    expect(result).toBe(false)
    expect(auth.token).toBeNull()
  })

  it('setup returns false on failure', async () => {
    const fetchMock = mockFetchSuccess({}, false)
    vi.stubGlobal('fetch', fetchMock)
    const auth = useAuthStore()

    const result = await auth.setup('admin', 'password123')

    expect(result).toBe(false)
    expect(auth.token).toBeNull()
  })

  it('checkSetupRequired throws on network error', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: false,
      status: 500,
      json: () => Promise.resolve({}),
    })
    vi.stubGlobal('fetch', fetchMock)
    const auth = useAuthStore()

    await expect(auth.checkSetupRequired()).rejects.toThrow('status check failed')
  })

  it('login sends credentials include', async () => {
    const fetchMock = mockFetchSuccess({ token: 'tok' })
    vi.stubGlobal('fetch', fetchMock)
    const auth = useAuthStore()

    await auth.login('user', 'pass')

    const [, options] = fetchMock.mock.calls[0]
    expect(options.credentials).toBe('include')
  })

  it('setup sends credentials include', async () => {
    const fetchMock = mockFetchSuccess({ token: 'tok' })
    vi.stubGlobal('fetch', fetchMock)
    const auth = useAuthStore()

    await auth.setup('user', 'password123')

    const [, options] = fetchMock.mock.calls[0]
    expect(options.credentials).toBe('include')
  })

  it('refresh sends credentials include', async () => {
    const fetchMock = mockFetchSuccess({ token: 'tok' })
    vi.stubGlobal('fetch', fetchMock)
    const auth = useAuthStore()

    await auth.refresh()

    const [, options] = fetchMock.mock.calls[0]
    expect(options.credentials).toBe('include')
  })

  it('setup failure does not change setupRequired', async () => {
    const fetchMock = mockFetchSuccess({}, false)
    vi.stubGlobal('fetch', fetchMock)
    const auth = useAuthStore()

    await auth.setup('admin', 'password123')

    // setupRequired should still be null (not changed on failure)
    expect(auth.setupRequired).toBeNull()
  })

  it('initializes token from localStorage', () => {
    localStorageMock['token'] = 'persisted-token'
    const auth = useAuthStore()
    expect(auth.token).toBe('persisted-token')
  })

  it('isAdminSession is true only with admin token', async () => {
    vi.stubGlobal('fetch', mockFetchSuccess({ token: 'abc123' }))
    const auth = useAuthStore()
    await auth.login('user', 'pass')
    expect(auth.isAdminSession).toBe(true)
    expect(auth.isApiKeySession).toBe(false)
  })

  it('isApiKeySession is true only with API key token', async () => {
    vi.stubGlobal(
      'fetch',
      mockFetchSuccess({ token: 'jwt-token', name: 'test-key', key_prefix: 'abc' }),
    )
    const auth = useAuthStore()
    await auth.loginWithApiKey('raw-key')
    expect(auth.isApiKeySession).toBe(true)
    expect(auth.isAdminSession).toBe(false)
  })

  it('loginWithApiKey stores JWT token in sessionStorage', async () => {
    vi.stubGlobal(
      'fetch',
      mockFetchSuccess({ token: 'jwt-token', name: 'my-key', key_prefix: 'xyz' }),
    )
    const auth = useAuthStore()
    const ok = await auth.loginWithApiKey('raw-key')
    expect(ok).toBe(true)
    expect(auth.apiKeyToken).toBe('jwt-token')
    expect(sessionStorageStub.setItem).toHaveBeenCalledWith('apiKeyToken', 'jwt-token')
    expect(auth.apiKeyInfo).toEqual({ name: 'my-key', key_prefix: 'xyz' })
  })

  it('loginWithApiKey returns false on failure', async () => {
    vi.stubGlobal('fetch', mockFetchSuccess({}, false))
    const auth = useAuthStore()
    const ok = await auth.loginWithApiKey('bad-key')
    expect(ok).toBe(false)
    expect(auth.apiKeyToken).toBeNull()
  })

  it('logoutApiKey clears API key session', async () => {
    vi.stubGlobal(
      'fetch',
      mockFetchSuccess({ token: 'jwt-token', name: 'k', key_prefix: 'x' }),
    )
    const auth = useAuthStore()
    await auth.loginWithApiKey('key')
    auth.logoutApiKey()
    expect(auth.apiKeyToken).toBeNull()
    expect(auth.apiKeyInfo).toBeNull()
    expect(auth.isApiKeySession).toBe(false)
    expect(sessionStorageStub.removeItem).toHaveBeenCalledWith('apiKeyToken')
  })

  it('isAuthenticated is true for either session type', async () => {
    vi.stubGlobal(
      'fetch',
      mockFetchSuccess({ token: 'jwt', name: 'k', key_prefix: 'x' }),
    )
    const auth = useAuthStore()
    expect(auth.isAuthenticated).toBe(false)
    await auth.loginWithApiKey('key')
    expect(auth.isAuthenticated).toBe(true)
  })

  it('logout clears both admin and API key sessions', async () => {
    vi.stubGlobal('fetch', mockFetchSuccess({ token: 'tok' }))
    const auth = useAuthStore()
    await auth.login('user', 'pass')
    auth.logout()
    expect(auth.isAdminSession).toBe(false)
    expect(auth.isApiKeySession).toBe(false)
    expect(auth.isAuthenticated).toBe(false)
  })

  it('initializes apiKeyToken from sessionStorage', () => {
    sessionStorageMock['apiKeyToken'] = 'persisted-jwt'
    const auth = useAuthStore()
    expect(auth.apiKeyToken).toBe('persisted-jwt')
    expect(auth.isApiKeySession).toBe(true)
  })
})
