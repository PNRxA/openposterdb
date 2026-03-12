import { test, expect } from '@playwright/test'

test.describe('landing page', () => {
  test('unauthenticated user sees landing page at /', async ({ page, request }) => {
    // Ensure admin exists so we don't get redirected to /setup
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })

    await page.goto('/')
    await expect(page.locator('h1')).toContainText('OpenPosterDB')
    await expect(page.locator('text=Self-hosted poster, logo, and backdrop serving')).toBeVisible()
  })

  test('landing page has sign in link that navigates to /login', async ({ page, request }) => {
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })

    await page.goto('/')
    const signInLink = page.locator('a:has-text("Sign in")')
    await expect(signInLink).toBeVisible()
    await signInLink.click()
    await expect(page).toHaveURL(/\/login/)
  })

  test('landing page has GitHub link', async ({ page, request }) => {
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })

    await page.goto('/')
    const ghLink = page.locator('a:has-text("GitHub")')
    await expect(ghLink).toBeVisible()
    await expect(ghLink).toHaveAttribute('href', /github\.com/)
  })

  test('landing page shows feature cards', async ({ page, request }) => {
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })

    await page.goto('/')
    await expect(page.locator('text=Posters & Backdrops')).toBeVisible()
    await expect(page.locator('text=API Key Management')).toBeVisible()
    await expect(page.locator('text=Fast & Cached')).toBeVisible()
    await expect(page.locator('text=RPDB Compatible')).toBeVisible()
  })

  test('authenticated admin visiting / gets redirected to /admin', async ({ page, request }) => {
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })

    await page.goto('/login')
    await page.fill('#username', 'admin')
    await page.fill('#password', 'testpassword123')
    await page.click('button[type="submit"]')
    await expect(page).toHaveURL(/\/admin/)

    // Navigate back to / — should redirect to /admin
    await page.goto('/')
    await expect(page).toHaveURL(/\/admin/)
  })

  test('login page has back to home link', async ({ page, request }) => {
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })

    await page.goto('/login')
    const backLink = page.locator('a:has-text("Back to home")')
    await expect(backLink).toBeVisible()
    await backLink.click()
    await expect(page.locator('h1')).toContainText('OpenPosterDB')
  })
})
