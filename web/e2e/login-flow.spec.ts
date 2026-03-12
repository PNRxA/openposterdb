import { test, expect } from '@playwright/test'

test.describe('login flow', () => {
  // Setup is assumed to be done by the setup-flow project dependency

  test('login with valid credentials', async ({ page, request }) => {
    // Ensure admin exists via API
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })

    await page.goto('/login')
    await page.fill('#username', 'admin')
    await page.fill('#password', 'testpassword123')
    await page.click('button[type="submit"]')

    await expect(page).toHaveURL(/\/admin/)
  })

  test('login with invalid credentials shows error', async ({ page, request }) => {
    // Ensure admin exists
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })

    await page.goto('/login')
    await page.fill('#username', 'admin')
    await page.fill('#password', 'wrongpassword')
    await page.click('button[type="submit"]')

    await expect(page.locator('.text-destructive')).toBeVisible()
  })

  test('logout redirects to /login', async ({ page, request }) => {
    // Setup and login via API
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })

    await page.goto('/login')
    await page.fill('#username', 'admin')
    await page.fill('#password', 'testpassword123')
    await page.click('button[type="submit"]')
    await expect(page).toHaveURL(/\/admin/)

    // Click logout in sidebar
    await page.click('text=Sign out')
    await expect(page).toHaveURL(/\/login/)
  })

  test('accessing /admin/keys when logged out redirects to /login', async ({ page }) => {
    await page.goto('/admin/keys')
    await expect(page).toHaveURL(/\/login|\/setup/)
  })
})
