import { test, expect } from '@playwright/test'
import { selectOption } from './helpers'

test.describe('key settings (self-service)', () => {
  /** Ensure admin exists and return an admin JWT token. */
  async function ensureAdmin(request: any): Promise<string> {
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })
    const loginRes = await request.post('/api/auth/login', {
      data: { username: 'admin', password: 'testpassword123' },
    })
    const { token } = await loginRes.json()
    return token
  }

  /** Create admin + API key, login with key via UI. */
  async function loginWithApiKey(
    page: any,
    request: any,
  ): Promise<string> {
    const token = await ensureAdmin(request)

    const keyRes = await request.post('/api/keys', {
      headers: { Authorization: `Bearer ${token}` },
      data: { name: 'settings-test-key' },
    })
    const keyData = await keyRes.json()
    const apiKey = keyData.key

    // Login via UI with API key
    await page.goto('/login')
    await page.click('text=Sign in with API key instead')
    await page.fill('#apikey', apiKey)
    await page.click('button[type="submit"]')
    await expect(page).toHaveURL(/\/key-settings/)

    return apiKey
  }

  test('displays settings form with defaults', async ({ page, request }) => {
    // Ensure global settings are at a known state (other tests may change them)
    const adminToken = await ensureAdmin(request)
    await request.put('/api/admin/settings', {
      headers: {
        Authorization: `Bearer ${adminToken}`,
        'Content-Type': 'application/json',
      },
      data: { image_source: 't' },
    })

    await loginWithApiKey(page, request)

    await expect(page.locator('h1')).toContainText('Image Settings')
    await expect(page.locator('text=settings-test-key')).toBeVisible()

    // Settings form should be present with fanart checkbox
    await expect(page.getByTestId('fanart-checkbox')).toBeVisible()
  })

  test('auto-saves and shows confirmation', async ({ page, request }) => {
    await loginWithApiKey(page, request)

    // Change a setting to trigger auto-save
    await page.getByTestId('fanart-checkbox').check()

    // Wait for auto-save confirmation
    await expect(page.locator('text=Saved')).toBeVisible({ timeout: 5000 })
  })

  test('language and textless options are always enabled', async ({ page, request }) => {
    await loginWithApiKey(page, request)

    await expect(page.getByTestId('lang-select')).toBeEnabled()
    await expect(page.getByTestId('textless-checkbox')).toBeEnabled()
  })

  test('settings persist after auto-save and reload', async ({ page, request }) => {
    await loginWithApiKey(page, request)

    // Enable fanart
    await page.getByTestId('fanart-checkbox').check()

    // Wait for auto-save confirmation
    await expect(page.locator('text=Saved')).toBeVisible({ timeout: 5000 })

    // Reload
    await page.reload()
    await expect(page.locator('h1')).toContainText('Image Settings')

    // Settings should persist
    await expect(page.getByTestId('fanart-checkbox')).toBeChecked()
  })

  test('rating display section is visible', async ({ page, request }) => {
    await loginWithApiKey(page, request)

    await expect(page.locator('text=Rating Display')).toBeVisible()
    await expect(page.locator('text=Rating order')).toBeVisible()
  })

  test('rating limit defaults to 3', async ({ page, request }) => {
    await loginWithApiKey(page, request)

    const limitInput = page.locator('#ratings-limit-self')
    await expect(limitInput).toBeVisible()
    await expect(limitInput).toHaveValue('3')
  })

  test('exclude ratings section is visible', async ({ page, request }) => {
    await loginWithApiKey(page, request)

    await expect(page.locator('text=Exclude ratings')).toBeVisible()
    await expect(page.getByTestId('exclude-rt-checkbox')).toBeVisible()
  })

  test('reset to defaults works', async ({ page, request }) => {
    // Ensure global settings are at a known state before testing reset.
    // Other tests (e.g. settings.spec.ts) may change global poster_source,
    // which would make the post-reset value unpredictable.
    const adminToken = await ensureAdmin(request)
    await request.put('/api/admin/settings', {
      headers: {
        Authorization: `Bearer ${adminToken}`,
        'Content-Type': 'application/json',
      },
      data: { image_source: 't' },
    })

    await loginWithApiKey(page, request)

    // Global is now "tmdb", key has no overrides → fanart should be unchecked
    await expect(page.getByTestId('fanart-checkbox')).not.toBeChecked()

    // Enable fanart — auto-save triggers
    await page.getByTestId('fanart-checkbox').check()
    await expect(page.locator('text=Saved')).toBeVisible({ timeout: 5000 })

    // Wait for "Using defaults" badge to disappear (confirms custom settings saved)
    await expect(page.locator('text=Using defaults')).not.toBeVisible()

    // Reset to defaults
    await page.locator('button:has-text("Reset to defaults")').click()

    // Should be back to global default (fanart unchecked)
    await expect(page.locator('text=Using defaults')).toBeVisible({ timeout: 10000 })
    await expect(page.getByTestId('fanart-checkbox')).not.toBeChecked()
  })

  test('reset to defaults does not trigger a spurious auto-save', async ({ page, request }) => {
    const adminToken = await ensureAdmin(request)
    await request.put('/api/admin/settings', {
      headers: {
        Authorization: `Bearer ${adminToken}`,
        'Content-Type': 'application/json',
      },
      data: { image_source: 't' },
    })

    await loginWithApiKey(page, request)

    // Change a setting so we have custom overrides
    await page.getByTestId('fanart-checkbox').check()
    await expect(page.locator('text=Saved')).toBeVisible({ timeout: 5000 })
    await expect(page.locator('text=Using defaults')).not.toBeVisible()

    // Start tracking network requests after clicking reset
    const requestsAfterReset: string[] = []
    await page.route('**/api/key/me/settings', (route) => {
      requestsAfterReset.push(route.request().method())
      route.continue()
    })

    // Click reset
    await page.locator('button:has-text("Reset to defaults")').click()

    // Should show "Using defaults" badge
    await expect(page.locator('text=Using defaults')).toBeVisible({ timeout: 10000 })

    // Wait for any auto-save to fire
    await page.waitForTimeout(1000)

    // Should have seen DELETE + GET, but no PUT
    const putCount = requestsAfterReset.filter(m => m === 'PUT').length
    expect(putCount).toBe(0)
  })

  test('poster position dropdown is visible', async ({ page, request }) => {
    await loginWithApiKey(page, request)

    // Exact match: the overlay-badge "Quality badge position" / "Main-language
    // badge position" labels also contain "Badge position".
    await expect(page.getByText('Badge position', { exact: true })).toBeVisible()
  })

  test('badge direction dropdown is visible with default', async ({ page, request }) => {
    await loginWithApiKey(page, request)

    const dirSelect = page.getByTestId('poster-badge-direction-select')
    await expect(dirSelect).toBeVisible()
    await expect(dirSelect).toContainText('Default')
  })

  test('badge direction persists after change and reload', async ({ page, request }) => {
    await loginWithApiKey(page, request)

    const dirSelect = page.getByTestId('poster-badge-direction-select')
    await selectOption(page, dirSelect, 'Horizontal')

    await expect(page.locator('text=Saved')).toBeVisible({ timeout: 5000 })

    await page.reload()
    await expect(page.locator('h1')).toContainText('Image Settings')
    await expect(page.getByTestId('poster-badge-direction-select')).toContainText('Horizontal')
  })

  test('label style dropdowns are visible', async ({ page, request }) => {
    await loginWithApiKey(page, request)

    // Check poster, logo, and backdrop label style selects
    for (const testId of ['poster-label-style-select', 'logo-label-style-select', 'backdrop-label-style-select']) {
      const select = page.getByTestId(testId)
      await expect(select).toBeVisible()
      await expect(select).toContainText('Official')
    }
  })

  test('label style persists after change and reload', async ({ page, request }) => {
    await loginWithApiKey(page, request)

    // Change poster label style to Text
    const labelSelect = page.getByTestId('poster-label-style-select')
    await selectOption(page, labelSelect, 'Text')

    // Wait for auto-save confirmation
    await expect(page.locator('text=Saved')).toBeVisible({ timeout: 5000 })

    // Reload and verify persistence
    await page.reload()
    await expect(page.locator('h1')).toContainText('Image Settings')

    await expect(page.getByTestId('poster-label-style-select')).toContainText('Text')
  })
})
