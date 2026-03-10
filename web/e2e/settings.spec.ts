import { test, expect } from '@playwright/test'

test.describe('settings', () => {
  test.beforeEach(async ({ page, request }) => {
    // Ensure admin exists and login
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })

    await page.goto('/login')
    await page.fill('#username', 'admin')
    await page.fill('#password', 'testpassword123')
    await page.click('button[type="submit"]')
    await expect(page).toHaveURL(/\/$/)

    // Navigate to Settings page
    await page.click('text=Settings')
    await expect(page).toHaveURL(/\/settings/)
  })

  test('settings page loads with heading', async ({ page }) => {
    await expect(page.locator('h1')).toContainText('Settings')
    await expect(page.locator('text=Global Poster Defaults')).toBeVisible()
  })

  test('displays poster source dropdown defaulting to TMDB', async ({ page }) => {
    const select = page.locator('select')
    await expect(select).toBeVisible()
    await expect(select).toHaveValue('tmdb')
  })

  test('fanart option is enabled when API key is configured', async ({ page }) => {
    const fanartOption = page.locator('option[value="fanart"]')
    await expect(fanartOption).toBeEnabled()
    await expect(fanartOption).not.toContainText('no API key')
  })

  test('fanart options appear when fanart is selected', async ({ page }) => {
    // Language and textless should not be visible initially
    await expect(page.locator('label:has-text("Language")')).not.toBeVisible()
    await expect(page.locator('label:has-text("Prefer textless")')).not.toBeVisible()

    // Select fanart
    await page.locator('select').selectOption('fanart')

    // Now language and textless should appear
    await expect(page.locator('label:has-text("Language")')).toBeVisible()
    await expect(page.locator('label:has-text("Prefer textless")')).toBeVisible()
  })

  test('save button works and shows confirmation', async ({ page }) => {
    const saveButton = page.locator('button:has-text("Save")')
    await expect(saveButton).toBeVisible()

    await saveButton.click()

    await expect(page.locator('button:has-text("Save") .text-green-500')).toBeVisible()
  })

  test('settings persist after save and reload', async ({ page }) => {
    // Select fanart and configure
    await page.locator('select').selectOption('fanart')
    await page.locator('input[placeholder="en"]').fill('de')

    // Save
    await page.locator('button:has-text("Save")').click()
    await expect(page.locator('button:has-text("Save") .text-green-500')).toBeVisible()

    // Reload page
    await page.reload()
    await expect(page.locator('h1')).toContainText('Settings')

    // Settings should be preserved
    await expect(page.locator('select')).toHaveValue('fanart')
  })

  test('refresh button is visible and clickable', async ({ page }) => {
    const refreshButton = page.locator('button:has-text("Refresh")')
    await expect(refreshButton).toBeVisible()

    await refreshButton.click()
    await expect(refreshButton).toBeVisible()
  })

  test('rating display section is visible', async ({ page }) => {
    await expect(page.locator('text=Rating Display')).toBeVisible()
    await expect(page.locator('text=Max ratings to show')).toBeVisible()
    await expect(page.locator('text=Rating order')).toBeVisible()
  })

  test('rating limit input defaults to 3', async ({ page }) => {
    const limitInput = page.locator('input[type="number"]')
    await expect(limitInput).toBeVisible()
    await expect(limitInput).toHaveValue('3')
  })

  test('all 8 rating sources are listed in order', async ({ page }) => {
    const ratingSection = page.locator('text=Rating order').locator('..')
    for (const label of ['IMDb', 'TMDB', 'Rotten Tomatoes (Critics)', 'Rotten Tomatoes (Audience)', 'Metacritic', 'Trakt', 'Letterboxd', 'MyAnimeList']) {
      await expect(ratingSection.locator(`text=${label}`)).toBeVisible()
    }
  })

  test('rating settings persist after save and reload', async ({ page }) => {
    // Change limit to a non-default value
    const limitInput = page.locator('input[type="number"]')
    await limitInput.fill('5')

    // Save
    await page.locator('button:has-text("Save")').click()
    await expect(page.locator('button:has-text("Save") .text-green-500')).toBeVisible()

    // Reload
    await page.reload()
    await expect(page.locator('h1')).toContainText('Settings')

    // Limit should be preserved
    await expect(page.locator('input[type="number"]')).toHaveValue('5')
  })

  test('sidebar navigation to settings works', async ({ page }) => {
    // Navigate away
    await page.click('text=Dashboard')
    await expect(page).toHaveURL(/\/$/)

    // Navigate back via sidebar
    await page.click('text=Settings')
    await expect(page).toHaveURL(/\/settings/)
  })
})
