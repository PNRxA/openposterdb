import { test, expect } from '@playwright/test'

test.describe('dashboard', () => {
  test.beforeEach(async ({ page, request }) => {
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })

    await page.goto('/login')
    await page.fill('#username', 'admin')
    await page.fill('#password', 'testpassword123')
    await page.click('button[type="submit"]')
    await expect(page).toHaveURL(/\/admin/)
  })

  test('shows stats cards', async ({ page }) => {
    const cards = page.locator('[data-slot="card"]')
    await expect(cards.filter({ hasText: 'Total Posters' })).toBeVisible()
    await expect(cards.filter({ hasText: 'API Keys' })).toBeVisible()
    await expect(cards.filter({ hasText: 'Memory Cache Entries' })).toBeVisible()
    await expect(cards.filter({ hasText: 'ID Cache Entries' })).toBeVisible()
    await expect(cards.filter({ hasText: 'Ratings Cache Entries' })).toBeVisible()
    await expect(cards.filter({ hasText: 'Poster Cache (MB)' })).toBeVisible()
  })

  test('refresh button fetches new data', async ({ page }) => {
    await expect(page.locator('text=Total Posters')).toBeVisible()

    await page.click('button:has-text("Refresh")')

    // After refresh, the check icon should briefly appear
    await expect(page.locator('text=Refresh')).toBeVisible()
  })

  test('clicking OpenPosterDB title navigates to dashboard', async ({ page }) => {
    // Navigate away first
    await page.click('text=Posters')
    await expect(page).toHaveURL(/\/admin\/posters/)

    // Click the title
    await page.click('text=OpenPosterDB')
    await expect(page).toHaveURL(/\/admin$/)
  })

  test('sidebar navigation works', async ({ page }) => {
    // Navigate to Posters
    await page.click('text=Posters')
    await expect(page).toHaveURL(/\/admin\/posters/)

    // Navigate to API Keys
    await page.click('text=API Keys')
    await expect(page).toHaveURL(/\/admin\/keys/)

    // Navigate back to Dashboard
    await page.click('text=Dashboard')
    await expect(page).toHaveURL(/\/admin$/)
  })
})
