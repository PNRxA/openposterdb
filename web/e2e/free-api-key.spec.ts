import { test, expect } from '@playwright/test'

test.describe('free API key', () => {
  /** Login as admin and navigate to settings. */
  async function loginAndGoToSettings(page: any, request: any) {
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })

    await page.goto('/login')
    await page.fill('#username', 'admin')
    await page.fill('#password', 'testpassword123')
    await page.click('button[type="submit"]')
    await expect(page).toHaveURL(/\/$/)

    await page.click('text=Settings')
    await expect(page).toHaveURL(/\/settings/)
  }

  /** Get admin JWT via API. */
  async function getAdminToken(request: any): Promise<string> {
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })
    const loginRes = await request.post('/api/auth/login', {
      data: { username: 'admin', password: 'testpassword123' },
    })
    const { token } = await loginRes.json()
    return token
  }

  test('free API key toggle appears on settings page', async ({ page, request }) => {
    await loginAndGoToSettings(page, request)

    await expect(page.locator('text=Free API Key')).toBeVisible()
    await expect(page.locator('button[role="switch"]')).toBeVisible()
  })

  test('free API key toggle defaults to disabled', async ({ page, request }) => {
    await loginAndGoToSettings(page, request)

    const toggle = page.locator('button[role="switch"]')
    await expect(toggle).toHaveAttribute('aria-checked', 'false')
  })

  test('toggle free API key on and verify persistence', async ({ page, request }) => {
    await loginAndGoToSettings(page, request)

    const toggle = page.locator('button[role="switch"]')
    await toggle.click()

    // Wait for the API call to complete
    await expect(toggle).toHaveAttribute('aria-checked', 'true')

    // Reload and verify
    await page.reload()
    await expect(page.locator('h1')).toContainText('Settings')
    await expect(page.locator('button[role="switch"]')).toHaveAttribute('aria-checked', 'true')
  })

  test('login page shows free key card when enabled', async ({ page, request }) => {
    const token = await getAdminToken(request)

    // Enable free API key via API
    await request.put('/api/admin/settings', {
      headers: { Authorization: `Bearer ${token}` },
      data: {
        poster_source: 't',
        free_api_key_enabled: true,
      },
    })

    // Visit login page (not logged in)
    await page.goto('/login')
    await expect(page.locator('text=Free API Key Available')).toBeVisible()
    await expect(page.locator('text=t0-free-rpdb')).toBeVisible()
  })

  test('login page hides free key card when disabled', async ({ page, request }) => {
    const token = await getAdminToken(request)

    // Ensure free API key is disabled
    await request.put('/api/admin/settings', {
      headers: { Authorization: `Bearer ${token}` },
      data: {
        poster_source: 't',
        free_api_key_enabled: false,
      },
    })

    await page.goto('/login')
    await expect(page.locator('text=Free API Key Available')).not.toBeVisible()
  })

  test('key-login with free API key returns 401', async ({ request }) => {
    const token = await getAdminToken(request)

    // Enable free API key
    await request.put('/api/admin/settings', {
      headers: { Authorization: `Bearer ${token}` },
      data: {
        poster_source: 't',
        free_api_key_enabled: true,
      },
    })

    // Try to login with the free key — should fail (no self-service)
    const res = await request.post('/api/auth/key-login', {
      data: { api_key: 't0-free-rpdb' },
    })
    expect(res.status()).toBe(401)
  })

  test('poster endpoint with free key works when enabled', async ({ request }) => {
    const token = await getAdminToken(request)

    // Enable free API key
    await request.put('/api/admin/settings', {
      headers: { Authorization: `Bearer ${token}` },
      data: {
        poster_source: 't',
        free_api_key_enabled: true,
      },
    })

    // Request a poster — it may fail at TMDB fetch, but should not be 401
    const res = await request.get('/t0-free-rpdb/imdb/poster-default/tt0111161.jpg')
    expect(res.status()).not.toBe(401)
  })

  test('poster endpoint with free key returns 401 when disabled', async ({ request }) => {
    const token = await getAdminToken(request)

    // Disable free API key
    await request.put('/api/admin/settings', {
      headers: { Authorization: `Bearer ${token}` },
      data: {
        poster_source: 't',
        free_api_key_enabled: false,
      },
    })

    const res = await request.get('/t0-free-rpdb/imdb/poster-default/tt0111161.jpg')
    expect(res.status()).toBe(401)
  })
})
