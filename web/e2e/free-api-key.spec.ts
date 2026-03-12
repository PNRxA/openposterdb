import { test, expect } from '@playwright/test'

const TEST_IMDB_ID = 'tt0111161'

/** Get admin JWT and enable free API key. */
async function enableFreeKey(request: any): Promise<string> {
  await request.post('/api/auth/setup', {
    data: { username: 'admin', password: 'testpassword123' },
  })
  const loginRes = await request.post('/api/auth/login', {
    data: { username: 'admin', password: 'testpassword123' },
  })
  const { token } = await loginRes.json()

  await request.put('/api/admin/settings', {
    headers: { Authorization: `Bearer ${token}` },
    data: { poster_source: 't', free_api_key_enabled: true },
  })

  return token
}

/** Disable free API key. */
async function disableFreeKey(request: any) {
  await request.post('/api/auth/setup', {
    data: { username: 'admin', password: 'testpassword123' },
  })
  const loginRes = await request.post('/api/auth/login', {
    data: { username: 'admin', password: 'testpassword123' },
  })
  const { token } = await loginRes.json()

  await request.put('/api/admin/settings', {
    headers: { Authorization: `Bearer ${token}` },
    data: { poster_source: 't', free_api_key_enabled: false },
  })
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

/** Check if real API keys are configured by attempting a poster fetch. */
async function hasRealKeys(request: any, token: string): Promise<boolean> {
  const keyRes = await request.post('/api/keys', {
    headers: { Authorization: `Bearer ${token}` },
    data: { name: 'key-check' },
  })
  const { key } = await keyRes.json()
  const res = await request.get(`/${key}/imdb/poster-default/${TEST_IMDB_ID}.jpg`, {
    timeout: 60_000,
  })
  return res.status() === 200
}

/** Login as admin and navigate to settings. */
async function loginAndGoToSettings(page: any, request: any) {
  await request.post('/api/auth/setup', {
    data: { username: 'admin', password: 'testpassword123' },
  })

  await page.goto('/login')
  await page.fill('#username', 'admin')
  await page.fill('#password', 'testpassword123')
  await page.click('button[type="submit"]')
  await expect(page).toHaveURL(/\/admin/)

  await page.click('text=Settings')
  await expect(page).toHaveURL(/\/admin\/settings/)
}

test.describe('free API key', () => {
  test('free API key toggle appears on settings page', async ({ page, request }) => {
    await loginAndGoToSettings(page, request)

    await expect(page.locator('text=Free API Key')).toBeVisible()
    await expect(page.locator('button[role="switch"]')).toBeVisible()
  })

  test('free API key toggle defaults to disabled', async ({ page, request }) => {
    // Explicitly disable free key to avoid interference from parallel tests
    const token = await getAdminToken(request)
    await request.put('/api/admin/settings', {
      headers: { Authorization: `Bearer ${token}` },
      data: { poster_source: 't', free_api_key_enabled: false },
    })

    await loginAndGoToSettings(page, request)

    const toggle = page.locator('button[role="switch"]')
    await expect(toggle).toHaveAttribute('aria-checked', 'false')
  })

  test('toggle free API key on and verify persistence', async ({ page, request }) => {
    // Start from known disabled state
    const token = await getAdminToken(request)
    await request.put('/api/admin/settings', {
      headers: { Authorization: `Bearer ${token}` },
      data: { poster_source: 't', free_api_key_enabled: false },
    })

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

test.describe('free API key card', () => {
  test('card is hidden when free key is disabled', async ({ page, request }) => {
    await disableFreeKey(request)

    await page.goto('/')
    await expect(page.locator('text=Free API Key Available')).not.toBeVisible()

    await page.goto('/login')
    await expect(page.locator('text=Free API Key Available')).not.toBeVisible()
  })

  test('card is visible on landing page when enabled', async ({ page, request }) => {
    await enableFreeKey(request)

    await page.goto('/')
    await expect(page.locator('text=Free API Key Available')).toBeVisible()
    await expect(page.locator('text=t0-free-rpdb')).toBeVisible()
  })

  test('card is visible on login page when enabled', async ({ page, request }) => {
    await enableFreeKey(request)

    await page.goto('/login')
    await expect(page.locator('text=Free API Key Available')).toBeVisible()
    await expect(page.locator('text=t0-free-rpdb')).toBeVisible()
  })

  test('try it out section is visible with form controls', async ({ page, request }) => {
    await enableFreeKey(request)

    await page.goto('/')
    await expect(page.locator('text=Try it out')).toBeVisible()

    // ID type select
    const idTypeSelect = page.locator('select').first()
    await expect(idTypeSelect).toBeVisible()
    await expect(idTypeSelect).toHaveValue('imdb')

    // Image type select
    const imageTypeSelect = page.locator('select').nth(1)
    await expect(imageTypeSelect).toBeVisible()
    await expect(imageTypeSelect).toHaveValue('poster')

    // ID input pre-filled with Nosferatu
    const idInput = page.locator('input[type="text"]')
    await expect(idInput).toBeVisible()
    await expect(idInput).toHaveValue('tt0013442')

    // Fetch button
    await expect(page.locator('button:has-text("Fetch")')).toBeVisible()
  })

  test('curl example updates when form values change', async ({ page, request }) => {
    await enableFreeKey(request)

    await page.goto('/')

    // Default curl should contain imdb and poster-default
    const curlBlock = page.locator('code:has-text("curl")')
    await expect(curlBlock).toContainText('imdb/poster-default/tt0013442.jpg')

    // Change ID type to TMDB
    await page.locator('select').first().selectOption('tmdb')
    await expect(curlBlock).toContainText('tmdb/poster-default/tt0013442.jpg')

    // Change image type to logo
    await page.locator('select').nth(1).selectOption('logo')
    await expect(curlBlock).toContainText('tmdb/logo-default/tt0013442.png')

    // Change ID value
    const idInput = page.locator('input[type="text"]')
    await idInput.clear()
    await idInput.fill('movie-278')
    await expect(curlBlock).toContainText('tmdb/logo-default/movie-278.png')
  })

  test('fetch poster shows image result', async ({ page, request }) => {
    const token = await enableFreeKey(request)

    if (!(await hasRealKeys(request, token))) {
      test.skip(true, 'Real API keys not configured')
      return
    }

    await page.goto('/')

    // Fill in a known IMDB ID
    const idInput = page.locator('input[type="text"]')
    await idInput.clear()
    await idInput.fill(TEST_IMDB_ID)

    // Click Fetch
    await page.locator('button:has-text("Fetch")').click()

    // Result image should appear
    const resultImg = page.locator('img[alt="Fetched result"]')
    await expect(resultImg).toBeVisible({ timeout: 60_000 })
  })

  test('fetch with invalid ID shows error', async ({ page, request }) => {
    const token = await enableFreeKey(request)

    if (!(await hasRealKeys(request, token))) {
      test.skip(true, 'Real API keys not configured')
      return
    }

    await page.goto('/')

    const idInput = page.locator('input[type="text"]')
    await idInput.clear()
    await idInput.fill('tt9999999')

    await page.locator('button:has-text("Fetch")').click()

    await expect(page.locator('text=Failed to fetch')).toBeVisible({ timeout: 60_000 })
  })

  test('fetch poster on login page works too', async ({ page, request }) => {
    const token = await enableFreeKey(request)

    if (!(await hasRealKeys(request, token))) {
      test.skip(true, 'Real API keys not configured')
      return
    }

    await page.goto('/login')

    const idInput = page.locator('input[type="text"]')
    await idInput.clear()
    await idInput.fill(TEST_IMDB_ID)

    await page.locator('button:has-text("Fetch")').click()

    const resultImg = page.locator('img[alt="Fetched result"]')
    await expect(resultImg).toBeVisible({ timeout: 60_000 })
  })

  test('switching image type and fetching shows correct result', async ({ page, request }) => {
    const token = await enableFreeKey(request)

    if (!(await hasRealKeys(request, token))) {
      test.skip(true, 'Real API keys not configured')
      return
    }

    await page.goto('/')

    const idInput = page.locator('input[type="text"]')
    await idInput.clear()
    await idInput.fill(TEST_IMDB_ID)

    // Fetch poster first
    await page.locator('button:has-text("Fetch")').click()
    const resultImg = page.locator('img[alt="Fetched result"]')
    await expect(resultImg).toBeVisible({ timeout: 60_000 })

    // Switch to backdrop and fetch again
    await page.locator('select').nth(1).selectOption('backdrop')
    await page.locator('button:has-text("Fetch")').click()

    // Image should still be visible (old stays until new loads)
    await expect(resultImg).toBeVisible({ timeout: 60_000 })
  })
})
