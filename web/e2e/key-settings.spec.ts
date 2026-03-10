import { test, expect } from '@playwright/test'

test.describe('key settings (self-service)', () => {
  /** Create admin + API key, login with key via UI. */
  async function loginWithApiKey(
    page: any,
    request: any,
  ): Promise<string> {
    // Ensure admin exists
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })

    // Login as admin to create key
    const loginRes = await request.post('/api/auth/login', {
      data: { username: 'admin', password: 'testpassword123' },
    })
    const { token } = await loginRes.json()

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
    await loginWithApiKey(page, request)

    await expect(page.locator('h1')).toContainText('Poster Settings')
    await expect(page.locator('text=settings-test-key')).toBeVisible()

    // Settings form should be present with defaults
    const select = page.locator('select')
    await expect(select).toBeVisible()
    await expect(select).toHaveValue('tmdb')
  })

  test('save settings shows confirmation', async ({ page, request }) => {
    await loginWithApiKey(page, request)

    const saveButton = page.locator('button:has-text("Save")')
    await expect(saveButton).toBeVisible()

    await saveButton.click()
    await expect(page.locator('button:has-text("Save") .text-green-500')).toBeVisible()
  })

  test('fanart options appear when fanart is selected', async ({ page, request }) => {
    await loginWithApiKey(page, request)

    // Language should not be visible initially
    await expect(page.locator('label:has-text("Language")')).not.toBeVisible()

    // Select fanart
    await page.locator('select').selectOption('fanart')

    // Now language and textless should appear
    await expect(page.locator('label:has-text("Language")')).toBeVisible()
    await expect(page.locator('label:has-text("Prefer textless")')).toBeVisible()
  })

  test('settings persist after save and reload', async ({ page, request }) => {
    await loginWithApiKey(page, request)

    // Change to fanart
    await page.locator('select').selectOption('fanart')
    await page.locator('input[placeholder="en"]').fill('de')

    // Save
    await page.locator('button:has-text("Save")').click()
    await expect(page.locator('button:has-text("Save") .text-green-500')).toBeVisible()

    // Reload
    await page.reload()
    await expect(page.locator('h1')).toContainText('Poster Settings')

    // Settings should persist
    await expect(page.locator('select')).toHaveValue('fanart')
  })

  test('rating display section is visible', async ({ page, request }) => {
    await loginWithApiKey(page, request)

    await expect(page.locator('text=Rating Display')).toBeVisible()
    await expect(page.locator('text=Max ratings to show')).toBeVisible()
    await expect(page.locator('text=Rating order')).toBeVisible()
  })

  test('rating limit defaults to 3', async ({ page, request }) => {
    await loginWithApiKey(page, request)

    const limitInput = page.locator('input[type="number"]')
    await expect(limitInput).toBeVisible()
    await expect(limitInput).toHaveValue('3')
  })

  test('reset to defaults works', async ({ page, request }) => {
    await loginWithApiKey(page, request)

    // Note the current (default) poster source
    const defaultSource = await page.locator('select').inputValue()

    // Change to something different
    const altSource = defaultSource === 'tmdb' ? 'fanart' : 'tmdb'
    await page.locator('select').selectOption(altSource)
    await page.locator('button:has-text("Save")').click()
    await expect(page.locator('button:has-text("Save") .text-green-500')).toBeVisible()

    // Wait for "Using defaults" badge to disappear (confirms custom settings saved)
    await expect(page.locator('text=Using defaults')).not.toBeVisible()

    // Reset to defaults
    await page.locator('button:has-text("Reset to defaults")').click()

    // Should be back to defaults
    await expect(page.locator('text=Using defaults')).toBeVisible({ timeout: 10000 })
    await expect(page.locator('select')).toHaveValue(defaultSource)
  })
})
