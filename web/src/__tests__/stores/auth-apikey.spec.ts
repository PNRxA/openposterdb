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

describe('auth store - API key session', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    Object.keys(localStorageMock).forEach((k) => delete localStorageMock[k])
    Object.keys(sessionStorageMock).forEach((k) => delete sessionStorageMock[k])
    vi.restoreAllMocks()
    vi.stubGlobal('localStorage', localStorageStub)
    vi.stubGlobal('sessionStorage', sessionStorageStub)
  })

  it('loginWithApiKey stores JWT token in sessionStorage on success', async () => {
    vi.stubGlobal(
      'fetch',
      mockFetchSuccess({ token: 'jwt-session-token', name: 'my-key', key_prefix: 'abcd1234' }),
    )
    const auth = useAuthStore()

    const result = await auth.loginWithApiKey('the-full-api-key')

    expect(result).toBe(true)
    expect(auth.apiKeyToken).toBe('jwt-session-token')
    expect(sessionStorageStub.setItem).toHaveBeenCalledWith('apiKeyToken', 'jwt-session-token')
    expect(auth.apiKeyInfo).toEqual({ name: 'my-key', key_prefix: 'abcd1234' })
  })

  it('loginWithApiKey returns false on failure', async () => {
    vi.stubGlobal('fetch', mockFetchSuccess({}, false))
    const auth = useAuthStore()

    const result = await auth.loginWithApiKey('bad-key')

    expect(result).toBe(false)
    expect(auth.apiKeyToken).toBeNull()
    expect(auth.apiKeyInfo).toBeNull()
  })

  it('isApiKeySession is true when apiKeyToken is set', async () => {
    vi.stubGlobal(
      'fetch',
      mockFetchSuccess({ token: 'jwt-tok', name: 'k', key_prefix: 'ab' }),
    )
    const auth = useAuthStore()
    await auth.loginWithApiKey('key123')

    expect(auth.isApiKeySession).toBe(true)
    expect(auth.isAdminSession).toBe(false)
    expect(auth.isAuthenticated).toBe(true)
  })

  it('isAdminSession is true when JWT token is set', async () => {
    vi.stubGlobal('fetch', mockFetchSuccess({ token: 'jwt-tok' }))
    const auth = useAuthStore()
    await auth.login('user', 'pass')

    expect(auth.isAdminSession).toBe(true)
    expect(auth.isApiKeySession).toBe(false)
    expect(auth.isAuthenticated).toBe(true)
  })

  it('logoutApiKey clears API key state', async () => {
    vi.stubGlobal(
      'fetch',
      mockFetchSuccess({ token: 'jwt-tok', name: 'k', key_prefix: 'ab' }),
    )
    const auth = useAuthStore()
    await auth.loginWithApiKey('key123')

    auth.logoutApiKey()

    expect(auth.apiKeyToken).toBeNull()
    expect(auth.apiKeyInfo).toBeNull()
    expect(auth.isApiKeySession).toBe(false)
    expect(auth.isAuthenticated).toBe(false)
    expect(sessionStorageStub.removeItem).toHaveBeenCalledWith('apiKeyToken')
  })

  it('logout clears both admin and API key state', async () => {
    vi.stubGlobal(
      'fetch',
      mockFetchSuccess({ token: 'jwt-tok', name: 'k', key_prefix: 'ab' }),
    )
    const auth = useAuthStore()
    await auth.loginWithApiKey('key123')

    auth.logout()

    expect(auth.apiKeyToken).toBeNull()
    expect(auth.token).toBeNull()
    expect(auth.isAuthenticated).toBe(false)
  })

  it('loginWithApiKey sends correct endpoint', async () => {
    const fetchMock = mockFetchSuccess({ name: 'k', key_prefix: 'ab' })
    vi.stubGlobal('fetch', fetchMock)
    const auth = useAuthStore()

    await auth.loginWithApiKey('my-key')

    const [url, options] = fetchMock.mock.calls[0]
    expect(url).toContain('/api/auth/key-login')
    expect(options.method).toBe('POST')
    expect(JSON.parse(options.body)).toEqual({ api_key: 'my-key' })
  })

  it('initializes apiKeyToken from sessionStorage', () => {
    sessionStorageMock['apiKeyToken'] = 'persisted-jwt'
    const auth = useAuthStore()
    expect(auth.apiKeyToken).toBe('persisted-jwt')
  })
})
