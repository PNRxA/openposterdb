import { test, expect } from '@playwright/test'

test.describe('api keys', () => {
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

    // Navigate to API Keys page
    await page.click('text=API Keys')
    await expect(page).toHaveURL(/\/keys/)
  })

  test('create a new API key and see its value', async ({ page }) => {
    await page.fill('input[placeholder*="Key name"]', 'test-key')
    await page.click('button:has-text("Create")')

    // Key value banner should appear
    await expect(page.locator('text=Copy your API key now')).toBeVisible()
    await expect(page.locator('code')).toBeVisible()
  })

  test('dismiss key banner', async ({ page }) => {
    await page.fill('input[placeholder*="Key name"]', 'dismiss-test')
    await page.click('button:has-text("Create")')

    await expect(page.locator('text=Copy your API key now')).toBeVisible()
    await page.click('button:has-text("Dismiss")')
    await expect(page.locator('text=Copy your API key now')).not.toBeVisible()
  })

  test('key appears in list with correct name', async ({ page }) => {
    await page.fill('input[placeholder*="Key name"]', 'my-prod-key')
    await page.click('button:has-text("Create")')

    // Key should appear in the list
    await expect(page.locator('text=my-prod-key')).toBeVisible()
  })

  test('refresh button is visible and clickable', async ({ page }) => {
    const refreshButton = page.locator('button:has-text("Refresh")')
    await expect(refreshButton).toBeVisible()

    await refreshButton.click()

    // Button should still be present after refresh completes
    await expect(refreshButton).toBeVisible()
  })

  /** Helper: create a key with a unique name and expand its settings panel. */
  let settingsKeyCounter = 0
  async function createKeyAndOpenSettings(page: any) {
    const keyName = `settings-key-${++settingsKeyCounter}-${Date.now()}`
    await page.fill('input[placeholder*="Key name"]', keyName)
    await page.click('button:has-text("Create")')
    await expect(page.getByText(keyName)).toBeVisible()

    // Dismiss the key banner if present
    const dismissBtn = page.locator('button:has-text("Dismiss")')
    if (await dismissBtn.isVisible()) await dismissBtn.click()

    // Click the settings gear button (the outline button with no text, next to Delete)
    const keyRow = page.getByText(keyName).locator('..').locator('..')
    await keyRow.locator('button:not(:has-text("Delete"))').first().click()

    // Wait for settings form to load
    await expect(page.locator('text=Rating Display')).toBeVisible()

    return keyName
  }

  test('per-key settings panel shows rating display section', async ({ page }) => {
    await createKeyAndOpenSettings(page)

    await expect(page.locator('text=Max ratings').first()).toBeVisible()
    await expect(page.locator('text=Rating order')).toBeVisible()
  })

  test('per-key rating limit defaults to 3', async ({ page }) => {
    await createKeyAndOpenSettings(page)

    // Target the poster max ratings input (first number input in the settings panel)
    const limitInput = page.locator('input[type="number"]').first()
    await expect(limitInput).toBeVisible()
    await expect(limitInput).toHaveValue('3')
  })

  test('per-key rating settings persist after auto-save', async ({ page }) => {
    const keyName = await createKeyAndOpenSettings(page)

    // Change limit to a non-default value (first number input = poster max ratings)
    const limitInput = page.locator('input[type="number"]').first()
    await limitInput.fill('5')

    // Wait for auto-save confirmation
    await expect(page.locator('text=Saved')).toBeVisible({ timeout: 5000 })

    // Collapse and re-expand settings to verify persistence
    const keyRow = page.getByText(keyName).locator('..').locator('..')
    await keyRow.locator('button:not(:has-text("Delete"))').first().click()
    await expect(page.locator('text=Rating Display')).not.toBeVisible()

    await keyRow.locator('button:not(:has-text("Delete"))').first().click()
    await expect(page.locator('text=Rating Display')).toBeVisible()

    await expect(page.locator('input[type="number"]').first()).toHaveValue('5')
  })

  test('per-key label style dropdowns are visible with default icon', async ({ page }) => {
    await createKeyAndOpenSettings(page)

    const labelSelects = page.locator('select').filter({ has: page.locator('option[value="icon"]') })
    // There should be 3 label style selects (poster, logo, backdrop)
    await expect(labelSelects).toHaveCount(3)

    for (const select of await labelSelects.all()) {
      await expect(select).toHaveValue('icon')
    }
  })

  test('per-key label style persists after auto-save', async ({ page }) => {
    const keyName = await createKeyAndOpenSettings(page)

    // Change poster label style to text (default is icon)
    const labelSelects = page.locator('select').filter({ has: page.locator('option[value="icon"]') })
    await labelSelects.first().selectOption('text')

    // Wait for auto-save confirmation
    await expect(page.locator('text=Saved')).toBeVisible({ timeout: 5000 })

    // Collapse and re-expand settings to verify persistence
    const keyRow = page.getByText(keyName).locator('..').locator('..')
    await keyRow.locator('button:not(:has-text("Delete"))').first().click()
    await expect(page.locator('text=Rating Display')).not.toBeVisible()

    await keyRow.locator('button:not(:has-text("Delete"))').first().click()
    await expect(page.locator('text=Rating Display')).toBeVisible()

    const reloadedSelects = page.locator('select').filter({ has: page.locator('option[value="icon"]') })
    await expect(reloadedSelects.first()).toHaveValue('text')
  })

  test('delete a key removes it from list', async ({ page }) => {
    // Create a key
    await page.fill('input[placeholder*="Key name"]', 'to-delete')
    await page.click('button:has-text("Create")')
    await expect(page.getByText('to-delete', { exact: true })).toBeVisible()

    // Dismiss the key banner if present
    const dismissBtn = page.locator('button:has-text("Dismiss")')
    if (await dismissBtn.isVisible()) await dismissBtn.click()

    // Accept the confirm dialog
    page.on('dialog', (dialog) => dialog.accept())

    // Click delete on the key's row
    const keyRow = page.getByText('to-delete', { exact: true }).locator('xpath=ancestor::div[contains(@class,"rounded-md")]')
    await keyRow.locator('button:has-text("Delete")').click()

    // Key should be gone
    await expect(page.getByText('to-delete', { exact: true })).not.toBeVisible()
  })
})
