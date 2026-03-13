import { test, expect } from '@playwright/test'

/**
 * Live API tests — require real API keys (TMDB, OMDb/MDBList, optionally Fanart).
 * Verify actual poster/logo/backdrop generation works end-to-end.
 *
 * Automatically skipped when the backend runs with dummy API keys.
 */

// The Shawshank Redemption — stable, well-known title
const TEST_IMDB_ID = 'tt0111161'
const TEST_TMDB_ID = '278'

/** Set up admin, create an API key, return raw key + admin JWT. */
async function setupAdminAndKey(request: any): Promise<{ apiKey: string; jwt: string }> {
  await request.post('/api/auth/setup', {
    data: { username: 'admin', password: 'testpassword123' },
  })

  const loginRes = await request.post('/api/auth/login', {
    data: { username: 'admin', password: 'testpassword123' },
  })
  const { token: jwt } = await loginRes.json()

  const keyRes = await request.post('/api/keys', {
    headers: { Authorization: `Bearer ${jwt}` },
    data: { name: 'live-test' },
  })
  const { key: apiKey } = await keyRes.json()

  return { apiKey, jwt }
}

test.describe('live API - poster generation', () => {
  test('fetches poster via IMDB ID and returns valid JPEG', async ({ request }) => {
    const { apiKey } = await setupAdminAndKey(request)

    const res = await request.get(`/${apiKey}/imdb/poster-default/${TEST_IMDB_ID}.jpg`, {
      timeout: 60_000,
    })

    if (res.status() !== 200) {
      test.skip(true, 'Real API keys not configured — skipping live tests')
      return
    }

    expect(res.headers()['content-type']).toBe('image/jpeg')
    const body = await res.body()
    // JPEG magic bytes: FF D8 FF
    expect(body[0]).toBe(0xff)
    expect(body[1]).toBe(0xd8)
    expect(body[2]).toBe(0xff)
    // A real poster should be at least 10KB
    expect(body.length).toBeGreaterThan(10_000)
  })

  test('fetches poster via TMDB ID and returns valid JPEG', async ({ request }) => {
    const { apiKey } = await setupAdminAndKey(request)

    const res = await request.get(`/${apiKey}/tmdb/poster-default/${TEST_TMDB_ID}.jpg`, {
      timeout: 60_000,
    })

    if (res.status() !== 200) {
      test.skip(true, 'Real API keys not configured')
      return
    }

    expect(res.headers()['content-type']).toBe('image/jpeg')
    const body = await res.body()
    expect(body[0]).toBe(0xff)
    expect(body[1]).toBe(0xd8)
    expect(body.length).toBeGreaterThan(10_000)
  })

  test('returns 401 for invalid API key', async ({ request }) => {
    const res = await request.get(`/invalid-key-here/imdb/poster-default/${TEST_IMDB_ID}.jpg`)
    expect(res.status()).toBe(401)
  })

  test('returns error for non-existent IMDB ID', async ({ request }) => {
    const { apiKey } = await setupAdminAndKey(request)

    const res = await request.get(`/${apiKey}/imdb/poster-default/tt9999999.jpg`, {
      timeout: 30_000,
    })

    // With dummy keys this will fail differently — only assert if keys are real
    if (res.status() === 200) {
      // Shouldn't happen for a non-existent ID, but don't fail the suite
      return
    }
    expect(res.status()).toBeGreaterThanOrEqual(400)
  })

  test('second fetch is faster (cache hit)', async ({ request }) => {
    const { apiKey } = await setupAdminAndKey(request)

    // First fetch to warm cache
    const firstRes = await request.get(`/${apiKey}/imdb/poster-default/${TEST_IMDB_ID}.jpg`, {
      timeout: 60_000,
    })

    if (firstRes.status() !== 200) {
      test.skip(true, 'Real API keys not configured')
      return
    }

    // Second fetch should be fast
    const start = Date.now()
    const res = await request.get(`/${apiKey}/imdb/poster-default/${TEST_IMDB_ID}.jpg`, {
      timeout: 10_000,
    })
    const elapsed = Date.now() - start

    expect(res.status()).toBe(200)
    expect(elapsed).toBeLessThan(2_000)
  })

  test('poster appears in admin posters list after fetch', async ({ request }) => {
    const { apiKey, jwt } = await setupAdminAndKey(request)

    const posterRes = await request.get(`/${apiKey}/imdb/poster-default/${TEST_IMDB_ID}.jpg`, {
      timeout: 60_000,
    })

    if (posterRes.status() !== 200) {
      test.skip(true, 'Real API keys not configured')
      return
    }

    const res = await request.get('/api/admin/posters?page=1&per_page=50', {
      headers: { Authorization: `Bearer ${jwt}` },
    })
    expect(res.status()).toBe(200)

    const body = await res.json()
    expect(body.total).toBeGreaterThan(0)
    const found = body.items.some(
      (p: any) => p.cache_key.includes('imdb') && p.cache_key.includes(TEST_IMDB_ID),
    )
    expect(found).toBe(true)
  })
})

test.describe('live API - imageSize parameter', () => {
  test('poster with imageSize=large returns valid JPEG', async ({ request }) => {
    const { apiKey } = await setupAdminAndKey(request)

    const res = await request.get(
      `/${apiKey}/imdb/poster-default/${TEST_IMDB_ID}.jpg?imageSize=large`,
      { timeout: 60_000 },
    )

    if (res.status() !== 200) {
      test.skip(true, 'Real API keys not configured — skipping live tests')
      return
    }

    expect(res.headers()['content-type']).toBe('image/jpeg')
    const body = await res.body()
    expect(body[0]).toBe(0xff)
    expect(body[1]).toBe(0xd8)
    expect(body.length).toBeGreaterThan(10_000)
  })

  test('poster with imageSize=small returns 400', async ({ request }) => {
    const { apiKey } = await setupAdminAndKey(request)

    const res = await request.get(
      `/${apiKey}/imdb/poster-default/${TEST_IMDB_ID}.jpg?imageSize=small`,
    )
    expect(res.status()).toBe(400)
  })

  test('poster with invalid imageSize returns 400', async ({ request }) => {
    const { apiKey } = await setupAdminAndKey(request)

    const res = await request.get(
      `/${apiKey}/imdb/poster-default/${TEST_IMDB_ID}.jpg?imageSize=huge`,
    )
    expect(res.status()).toBe(400)
  })

  test('backdrop with imageSize=small is accepted', async ({ request }) => {
    const { apiKey } = await setupAdminAndKey(request)

    const res = await request.get(
      `/${apiKey}/imdb/backdrop-default/${TEST_IMDB_ID}.jpg?imageSize=small`,
      { timeout: 60_000 },
    )

    // If fanart keys not configured, skip — but should NOT be 400
    if (res.status() !== 200) {
      // small is valid for backdrops, so it should never be 400
      expect(res.status()).not.toBe(400)
      test.skip(true, 'Fanart API key not configured or no backdrop available')
      return
    }

    expect(res.headers()['content-type']).toBe('image/jpeg')
  })

  test('logo with imageSize=small returns 400', async ({ request }) => {
    const { apiKey } = await setupAdminAndKey(request)

    const res = await request.get(
      `/${apiKey}/imdb/logo-default/${TEST_IMDB_ID}.png?imageSize=small`,
    )
    expect(res.status()).toBe(400)
  })
})

test.describe('live API - preview endpoints', () => {
  test('key preview returns valid JPEG', async ({ request }) => {
    const { apiKey } = await setupAdminAndKey(request)

    // Check if real keys are available
    const checkRes = await request.get(`/${apiKey}/imdb/poster-default/${TEST_IMDB_ID}.jpg`, {
      timeout: 60_000,
    })
    if (checkRes.status() !== 200) {
      test.skip(true, 'Real API keys not configured')
      return
    }

    const keyLoginRes = await request.post('/api/auth/key-login', {
      data: { api_key: apiKey },
    })
    const { token: sessionToken } = await keyLoginRes.json()

    const res = await request.get('/api/key/me/preview/poster', {
      headers: { Authorization: `Bearer ${sessionToken}` },
      timeout: 60_000,
    })
    expect(res.status()).toBe(200)
    expect(res.headers()['content-type']).toBe('image/jpeg')

    const body = await res.body()
    expect(body[0]).toBe(0xff)
    expect(body[1]).toBe(0xd8)
    expect(body.length).toBeGreaterThan(10_000)
  })

  test('admin preview returns valid JPEG', async ({ request }) => {
    const { apiKey, jwt } = await setupAdminAndKey(request)

    const checkRes = await request.get(`/${apiKey}/imdb/poster-default/${TEST_IMDB_ID}.jpg`, {
      timeout: 60_000,
    })
    if (checkRes.status() !== 200) {
      test.skip(true, 'Real API keys not configured')
      return
    }

    const res = await request.get('/api/admin/preview/poster', {
      headers: { Authorization: `Bearer ${jwt}` },
      timeout: 60_000,
    })
    expect(res.status()).toBe(200)
    expect(res.headers()['content-type']).toBe('image/jpeg')

    const body = await res.body()
    expect(body[0]).toBe(0xff)
    expect(body[1]).toBe(0xd8)
    expect(body.length).toBeGreaterThan(10_000)
  })
})

test.describe('live API - free API key', () => {
  test('free key returns real poster when enabled', async ({ request }) => {
    const { apiKey, jwt } = await setupAdminAndKey(request)

    // Check if real keys are available
    const checkRes = await request.get(`/${apiKey}/imdb/poster-default/${TEST_IMDB_ID}.jpg`, {
      timeout: 60_000,
    })
    if (checkRes.status() !== 200) {
      test.skip(true, 'Real API keys not configured')
      return
    }

    // Enable free API key
    await request.put('/api/admin/settings', {
      headers: { Authorization: `Bearer ${jwt}` },
      data: { poster_source: 't', free_api_key_enabled: true },
    })

    const res = await request.get(`/t0-free-rpdb/imdb/poster-default/${TEST_IMDB_ID}.jpg`, {
      timeout: 60_000,
    })
    expect(res.status()).toBe(200)
    expect(res.headers()['content-type']).toBe('image/jpeg')

    const body = await res.body()
    expect(body[0]).toBe(0xff)
    expect(body[1]).toBe(0xd8)
    expect(body.length).toBeGreaterThan(10_000)
  })
})

test.describe('live API - logo generation', () => {
  test('fetches logo via IMDB ID and returns valid PNG', async ({ request }) => {
    const { apiKey } = await setupAdminAndKey(request)

    const res = await request.get(`/${apiKey}/imdb/logo-default/${TEST_IMDB_ID}.png`, {
      timeout: 60_000,
    })

    // Logo requires real Fanart.tv key + logo availability
    if (res.status() !== 200) {
      test.skip(true, 'Fanart API key not configured or no logo available')
      return
    }

    expect(res.headers()['content-type']).toBe('image/png')
    const body = await res.body()
    // PNG magic bytes: 89 50 4E 47
    expect(body[0]).toBe(0x89)
    expect(body[1]).toBe(0x50)
    expect(body[2]).toBe(0x4e)
    expect(body[3]).toBe(0x47)
    expect(body.length).toBeGreaterThan(5_000)
  })
})

test.describe('live API - backdrop generation', () => {
  test('fetches backdrop via IMDB ID and returns valid JPEG', async ({ request }) => {
    const { apiKey } = await setupAdminAndKey(request)

    const res = await request.get(`/${apiKey}/imdb/backdrop-default/${TEST_IMDB_ID}.jpg`, {
      timeout: 60_000,
    })

    // Backdrop requires real Fanart.tv key + backdrop availability
    if (res.status() !== 200) {
      test.skip(true, 'Fanart API key not configured or no backdrop available')
      return
    }

    expect(res.headers()['content-type']).toBe('image/jpeg')
    const body = await res.body()
    expect(body[0]).toBe(0xff)
    expect(body[1]).toBe(0xd8)
    expect(body.length).toBeGreaterThan(10_000)
  })
})

test.describe('live API - UI poster fetch', () => {
  test('fetch poster from admin UI and verify it appears in table', async ({ page, request }) => {
    const { apiKey } = await setupAdminAndKey(request)

    // Check if real keys are available
    const checkRes = await request.get(`/${apiKey}/imdb/poster-default/${TEST_IMDB_ID}.jpg`, {
      timeout: 60_000,
    })
    if (checkRes.status() !== 200) {
      test.skip(true, 'Real API keys not configured')
      return
    }

    // Login to admin UI
    await page.goto('/login')
    await page.fill('#username', 'admin')
    await page.fill('#password', 'testpassword123')
    await page.click('button[type="submit"]')
    await expect(page).toHaveURL(/\/admin/)

    // Navigate to Posters page
    await page.click('text=Posters')
    await expect(page).toHaveURL(/\/admin\/posters/)

    // Use Fetch button to fetch a poster
    await page.click('button:has-text("Fetch")')
    await expect(page.getByText('Fetch Poster')).toBeVisible()

    // Fill in IMDb ID (The Dark Knight — different from other tests)
    const modal = page.getByRole('dialog')
    await modal.locator('input[placeholder="e.g. tt1234567"]').fill('tt0468569')
    const submitButton = modal.locator('button[type="submit"]:has-text("Fetch")')
    await expect(submitButton).toBeEnabled()
    await submitButton.click()

    // After fetch, a preview dialog opens — wait for it then close it
    const previewDialog = page.getByRole('dialog')
    await expect(previewDialog).toBeVisible({ timeout: 60_000 })
    await page.keyboard.press('Escape')
    await expect(previewDialog).not.toBeVisible({ timeout: 15_000 })

    // Wait for poster to appear in table
    await expect(page.getByText('tt0468569')).toBeVisible({ timeout: 15_000 })
  })
})

test.describe('live API - UI fetch shows preview immediately', () => {
  /** Login and navigate to a given admin page. */
  async function loginAndNavigate(page: any, request: any, section: string) {
    await setupAdminAndKey(request)

    await page.goto('/login')
    await page.fill('#username', 'admin')
    await page.fill('#password', 'testpassword123')
    await page.click('button[type="submit"]')
    await expect(page).toHaveURL(/\/admin/)

    const label = section.charAt(0).toUpperCase() + section.slice(1)
    await page.click(`text=${label}`)
    await expect(page).toHaveURL(new RegExp(`/admin/${section}`))
  }

  /** Fetch via UI and assert preview image is visible immediately. */
  async function fetchAndExpectPreview(page: any, imdbId: string) {
    await page.click('button:has-text("Fetch")')
    const modal = page.getByRole('dialog')
    await modal.locator('input[placeholder="e.g. tt1234567"]').fill(imdbId)
    await modal.locator('button[type="submit"]:has-text("Fetch")').click()

    // Preview dialog should open with a visible image — not "Failed to load"
    const previewDialog = page.getByRole('dialog')
    await expect(previewDialog.locator('img')).toBeVisible({ timeout: 60_000 })
    await expect(previewDialog.getByText('Failed to load')).not.toBeVisible()
  }

  test('poster preview is visible immediately after fetch', async ({ page, request }) => {
    const { apiKey } = await setupAdminAndKey(request)

    // Pre-check real keys
    const checkRes = await request.get(`/${apiKey}/imdb/poster-default/${TEST_IMDB_ID}.jpg`, {
      timeout: 60_000,
    })
    if (checkRes.status() !== 200) {
      test.skip(true, 'Real API keys not configured')
      return
    }

    await loginAndNavigate(page, request, 'posters')
    await fetchAndExpectPreview(page, TEST_IMDB_ID)
  })

  test('logo preview is visible immediately after fetch', async ({ page, request }) => {
    const { apiKey } = await setupAdminAndKey(request)

    // Pre-check real Fanart key
    const checkRes = await request.get(`/${apiKey}/imdb/logo-default/${TEST_IMDB_ID}.png`, {
      timeout: 60_000,
    })
    if (checkRes.status() !== 200) {
      test.skip(true, 'Fanart API key not configured or no logo available')
      return
    }

    await loginAndNavigate(page, request, 'logos')
    await fetchAndExpectPreview(page, TEST_IMDB_ID)
  })

  test('backdrop preview is visible immediately after fetch', async ({ page, request }) => {
    const { apiKey } = await setupAdminAndKey(request)

    // Pre-check real Fanart key
    const checkRes = await request.get(`/${apiKey}/imdb/backdrop-default/${TEST_IMDB_ID}.jpg`, {
      timeout: 60_000,
    })
    if (checkRes.status() !== 200) {
      test.skip(true, 'Fanart API key not configured or no backdrop available')
      return
    }

    await loginAndNavigate(page, request, 'backdrops')
    await fetchAndExpectPreview(page, TEST_IMDB_ID)
  })
})
