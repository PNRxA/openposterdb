import { test, expect } from '@playwright/test'

test.describe('API key login flow', () => {
  /** Create an admin + API key via the API, return the raw key. */
  async function createApiKey(request: typeof test extends (
    ...args: infer _
  ) => void
    ? never
    : any): Promise<string> {
    // Ensure admin exists
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })

    // Login to get JWT
    const loginRes = await request.post('/api/auth/login', {
      data: { username: 'admin', password: 'testpassword123' },
    })
    const { token } = await loginRes.json()

    // Create API key
    const keyRes = await request.post('/api/keys', {
      headers: { Authorization: `Bearer ${token}` },
      data: { name: 'e2e-test-key' },
    })
    const keyData = await keyRes.json()
    return keyData.key
  }

  test('login with valid API key navigates to /key-settings', async ({ page, request }) => {
    const apiKey = await createApiKey(request)

    await page.goto('/login')

    // Switch to API key mode
    await page.click('text=Sign in with API key instead')
    await page.fill('#apikey', apiKey)
    await page.click('button[type="submit"]')

    await expect(page).toHaveURL(/\/key-settings/)
    await expect(page.locator('h1')).toContainText('Poster Settings')
    await expect(page.locator('text=e2e-test-key')).toBeVisible()
  })

  test('login with invalid API key shows error', async ({ page, request }) => {
    // Ensure admin exists so setup redirect doesn't interfere
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })

    await page.goto('/login')
    await page.click('text=Sign in with API key instead')
    await page.fill('#apikey', 'totally-bogus-key')
    await page.click('button[type="submit"]')

    await expect(page.locator('.text-destructive')).toBeVisible()
    await expect(page.locator('.text-destructive')).toContainText('Invalid API key')
  })

  test('toggle between admin and API key mode', async ({ page, request }) => {
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })

    await page.goto('/login')

    // Initially admin mode
    await expect(page.locator('#username')).toBeVisible()
    await expect(page.locator('#password')).toBeVisible()

    // Switch to API key mode
    await page.click('text=Sign in with API key instead')
    await expect(page.locator('#apikey')).toBeVisible()
    await expect(page.locator('#username')).not.toBeVisible()

    // Switch back
    await page.click('text=Sign in as admin instead')
    await expect(page.locator('#username')).toBeVisible()
    await expect(page.locator('#apikey')).not.toBeVisible()
  })

  test('API key session cannot access admin dashboard', async ({ page, request }) => {
    const apiKey = await createApiKey(request)

    await page.goto('/login')
    await page.click('text=Sign in with API key instead')
    await page.fill('#apikey', apiKey)
    await page.click('button[type="submit"]')
    await expect(page).toHaveURL(/\/key-settings/)

    // Try to navigate to admin area
    await page.goto('/admin')
    await expect(page).toHaveURL(/\/key-settings/)
  })

  test('logout from key-settings redirects to login', async ({ page, request }) => {
    const apiKey = await createApiKey(request)

    await page.goto('/login')
    await page.click('text=Sign in with API key instead')
    await page.fill('#apikey', apiKey)
    await page.click('button[type="submit"]')
    await expect(page).toHaveURL(/\/key-settings/)

    await page.click('button:has-text("Logout")')
    await expect(page).toHaveURL(/\/login/)
  })
})
