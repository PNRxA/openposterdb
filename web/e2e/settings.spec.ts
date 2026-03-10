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

    await expect(page.locator('text=Saved')).toBeVisible()
  })

  test('settings persist after save and reload', async ({ page }) => {
    // Select fanart and configure
    await page.locator('select').selectOption('fanart')
    await page.locator('input[placeholder="en"]').fill('de')

    // Save
    await page.locator('button:has-text("Save")').click()
    await expect(page.locator('text=Saved')).toBeVisible()

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

  test('sidebar navigation to settings works', async ({ page }) => {
    // Navigate away
    await page.click('text=Dashboard')
    await expect(page).toHaveURL(/\/$/)

    // Navigate back via sidebar
    await page.click('text=Settings')
    await expect(page).toHaveURL(/\/settings/)
  })
})
