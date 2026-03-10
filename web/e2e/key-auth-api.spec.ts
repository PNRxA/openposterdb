import { test, expect } from '@playwright/test'

test.describe('API key auth - API level security', () => {
  /** Create admin + API key, return raw key, session JWT, and admin JWT. */
  async function setupKeyAndAdmin(
    request: any,
  ): Promise<{ apiKey: string; sessionToken: string; jwt: string }> {
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })

    const loginRes = await request.post('/api/auth/login', {
      data: { username: 'admin', password: 'testpassword123' },
    })
    const { token: jwt } = await loginRes.json()

    const keyRes = await request.post('/api/keys', {
      headers: { Authorization: `Bearer ${jwt}` },
      data: { name: 'security-test' },
    })
    const { key: apiKey } = await keyRes.json()

    // Login with the API key to get a session JWT
    const keyLoginRes = await request.post('/api/auth/key-login', {
      data: { api_key: apiKey },
    })
    const { token: sessionToken } = await keyLoginRes.json()

    return { apiKey, sessionToken, jwt }
  }

  test('POST /api/auth/key-login with valid key returns token and info', async ({ request }) => {
    const { apiKey } = await setupKeyAndAdmin(request)

    const res = await request.post('/api/auth/key-login', {
      data: { api_key: apiKey },
    })
    expect(res.status()).toBe(200)

    const body = await res.json()
    expect(body.name).toBe('security-test')
    expect(body.key_prefix).toBeTruthy()
    expect(body.token).toBeTruthy()
  })

  test('POST /api/auth/key-login with invalid key returns 401', async ({ request }) => {
    // Ensure admin exists
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })

    const res = await request.post('/api/auth/key-login', {
      data: { api_key: 'nonexistent-key' },
    })
    expect(res.status()).toBe(401)
  })

  test('GET /api/key/me with session JWT returns info', async ({ request }) => {
    const { sessionToken } = await setupKeyAndAdmin(request)

    const res = await request.get('/api/key/me', {
      headers: { Authorization: `Bearer ${sessionToken}` },
    })
    expect(res.status()).toBe(200)

    const body = await res.json()
    expect(body.name).toBe('security-test')
  })

  test('GET /api/key/me without auth returns 401', async ({ request }) => {
    const res = await request.get('/api/key/me')
    expect(res.status()).toBe(401)
  })

  test('GET /api/key/me with raw API key returns 401', async ({ request }) => {
    const { apiKey } = await setupKeyAndAdmin(request)

    const res = await request.get('/api/key/me', {
      headers: { Authorization: `Bearer ${apiKey}` },
    })
    expect(res.status()).toBe(401)
  })

  test('GET /api/key/me/settings returns defaults', async ({ request }) => {
    const { sessionToken } = await setupKeyAndAdmin(request)

    const res = await request.get('/api/key/me/settings', {
      headers: { Authorization: `Bearer ${sessionToken}` },
    })
    expect(res.status()).toBe(200)

    const body = await res.json()
    expect(body.poster_source).toBe('tmdb')
    expect(body.is_default).toBe(true)
  })

  test('PUT /api/key/me/settings updates and reads back', async ({ request }) => {
    const { sessionToken } = await setupKeyAndAdmin(request)

    const putRes = await request.put('/api/key/me/settings', {
      headers: { Authorization: `Bearer ${sessionToken}` },
      data: { poster_source: 'fanart', fanart_lang: 'ja', fanart_textless: true },
    })
    expect(putRes.status()).toBe(200)

    const getRes = await request.get('/api/key/me/settings', {
      headers: { Authorization: `Bearer ${sessionToken}` },
    })
    const body = await getRes.json()
    expect(body.poster_source).toBe('fanart')
    expect(body.fanart_lang).toBe('ja')
    expect(body.fanart_textless).toBe(true)
    expect(body.is_default).toBe(false)
  })

  test('DELETE /api/key/me/settings resets to defaults', async ({ request }) => {
    const { sessionToken } = await setupKeyAndAdmin(request)

    // Set custom
    await request.put('/api/key/me/settings', {
      headers: { Authorization: `Bearer ${sessionToken}` },
      data: { poster_source: 'fanart', fanart_lang: 'de', fanart_textless: true },
    })

    // Reset
    const delRes = await request.delete('/api/key/me/settings', {
      headers: { Authorization: `Bearer ${sessionToken}` },
    })
    expect(delRes.status()).toBe(200)

    // Verify defaults
    const getRes = await request.get('/api/key/me/settings', {
      headers: { Authorization: `Bearer ${sessionToken}` },
    })
    const body = await getRes.json()
    expect(body.poster_source).toBe('tmdb')
    expect(body.is_default).toBe(true)
  })

  test('API key session cannot access admin endpoints', async ({ request }) => {
    const { sessionToken } = await setupKeyAndAdmin(request)

    const adminEndpoints = [
      { method: 'GET', path: '/api/keys' },
      { method: 'GET', path: '/api/admin/stats' },
      { method: 'GET', path: '/api/admin/settings' },
    ]

    for (const { method, path } of adminEndpoints) {
      const res = await (request as any)[method.toLowerCase()](path, {
        headers: { Authorization: `Bearer ${sessionToken}` },
      })
      expect(res.status()).toBe(401)
    }
  })

  test('Admin JWT cannot access self-service endpoints', async ({ request }) => {
    const { jwt } = await setupKeyAndAdmin(request)

    const res = await request.get('/api/key/me', {
      headers: { Authorization: `Bearer ${jwt}` },
    })
    expect(res.status()).toBe(401)
  })
})
