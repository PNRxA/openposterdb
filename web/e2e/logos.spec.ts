import { test, expect } from '@playwright/test'

test.describe('logos page', () => {
  test.beforeEach(async ({ page, request }) => {
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })

    await page.goto('/login')
    await page.fill('#username', 'admin')
    await page.fill('#password', 'testpassword123')
    await page.click('button[type="submit"]')
    await expect(page).toHaveURL(/\/admin/)

    await page.click('text=Logos')
    await expect(page).toHaveURL(/\/admin\/logos/)
  })

  test('shows table headers', async ({ page }) => {
    await expect(page.locator('th:has-text("ID Type")')).toBeVisible()
    await expect(page.locator('th:has-text("ID Value")')).toBeVisible()
    await expect(page.locator('th:has-text("Release Date")')).toBeVisible()
    await expect(page.locator('th:has-text("Last Updated")')).toBeVisible()
    await expect(page.locator('th:has-text("Created")')).toBeVisible()
  })

  test('shows empty state when no logos', async ({ page }) => {
    await expect(page.getByRole('cell', { name: 'No logos cached yet.' })).toBeVisible({ timeout: 15_000 })
    await expect(page.getByText('0 logos total')).toBeVisible()
  })

  test('has refresh button', async ({ page }) => {
    await expect(page.locator('button:has-text("Refresh")')).toBeVisible()
  })

  test('has pagination controls', async ({ page }) => {
    await expect(page.locator('button:has-text("Previous")')).toBeVisible()
    await expect(page.locator('button:has-text("Next")')).toBeVisible()
    await expect(page.locator('text=/Page \\d+ of \\d+/')).toBeVisible()
  })

  test('has fetch button', async ({ page }) => {
    await expect(page.locator('button:has-text("Fetch")')).toBeVisible()
  })

  test('fetch button opens modal with form', async ({ page }) => {
    await page.click('button:has-text("Fetch")')
    await expect(page.getByText('Fetch Logo')).toBeVisible()
    const modal = page.getByRole('dialog')
    await expect(modal.getByText('ID Type')).toBeVisible()
    await expect(modal.getByText('ID Value')).toBeVisible()
    await expect(modal.locator('select')).toBeVisible()
    await expect(modal.locator('input[placeholder="e.g. tt1234567"]')).toBeVisible()
  })

  test('fetch modal has correct id type options', async ({ page }) => {
    await page.click('button:has-text("Fetch")')
    const select = page.locator('select')
    await expect(select.locator('option[value="imdb"]')).toHaveText('IMDb')
    await expect(select.locator('option[value="tmdb"]')).toHaveText('TMDb')
    await expect(select.locator('option[value="tvdb"]')).toHaveText('TVDB')
  })

  test('fetch submit button is disabled when input is empty', async ({ page }) => {
    await page.click('button:has-text("Fetch")')
    const submitButton = page.locator('button[type="submit"]:has-text("Fetch")')
    await expect(submitButton).toBeDisabled()
  })

  test('fetch submit button is enabled when input has value', async ({ page }) => {
    await page.click('button:has-text("Fetch")')
    await page.fill('input[placeholder="e.g. tt1234567"]', 'tt0111161')
    const submitButton = page.locator('button[type="submit"]:has-text("Fetch")')
    await expect(submitButton).toBeEnabled()
  })
})
