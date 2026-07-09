import { test, expect } from '@playwright/test'
import { selectOption } from './helpers'

test.describe('settings', () => {
  test.beforeEach(async ({ page, request }) => {
    // Ensure admin exists and login
    await request.post('/api/auth/setup', {
      data: { username: 'admin', password: 'testpassword123' },
    })

    const loginRes = await request.post('/api/auth/login', {
      data: { username: 'admin', password: 'testpassword123' },
    })
    const { token } = await loginRes.json()

    // Reset settings to defaults so tests start from a known state
    await request.put('/api/admin/settings', {
      headers: { Authorization: `Bearer ${token}` },
      data: { image_source: 't' },
    })

    await page.goto('/login')
    await page.fill('#username', 'admin')
    await page.fill('#password', 'testpassword123')
    await page.click('button[type="submit"]')
    await expect(page).toHaveURL(/\/admin/)

    // Navigate to Settings page
    await page.click('text=Settings')
    await expect(page).toHaveURL(/\/admin\/settings/)
  })

  test('settings page loads with heading', async ({ page }) => {
    await expect(page.locator('h1')).toContainText('Settings')
    await expect(page.locator('text=Global Image Settings')).toBeVisible()
  })

  test('fanart checkbox is visible', async ({ page }) => {
    await expect(page.getByTestId('fanart-checkbox')).toBeVisible()
  })

  test('language and textless options are always enabled', async ({ page }) => {
    await expect(page.getByTestId('lang-select')).toBeVisible()
    await expect(page.getByTestId('lang-select')).toBeEnabled()
    await expect(page.getByTestId('textless-checkbox')).toBeVisible()
    await expect(page.getByTestId('textless-checkbox')).toBeEnabled()
  })

  test('enabling fanart auto-saves', async ({ page }) => {
    await page.getByTestId('fanart-checkbox').check()

    // Wait for auto-save confirmation
    await expect(page.locator('text=Saved')).toBeVisible({ timeout: 5000 })
  })

  test('fanart options persist after auto-save and reload', async ({ page }) => {
    // Enable fanart and textless
    await page.getByTestId('fanart-checkbox').check()
    await page.getByTestId('textless-checkbox').check()

    // Wait for auto-save confirmation
    await expect(page.locator('text=Saved')).toBeVisible({ timeout: 5000 })

    // Reload page
    await page.reload()
    await expect(page.locator('h1')).toContainText('Settings')

    // Fanart and textless should be checked
    await expect(page.getByTestId('fanart-checkbox')).toBeChecked()
    await expect(page.getByTestId('textless-checkbox')).toBeChecked()
  })

  test('refresh button is visible and clickable', async ({ page }) => {
    const refreshButton = page.locator('button:has-text("Refresh")')
    await expect(refreshButton).toBeVisible()

    await refreshButton.click()
    await expect(refreshButton).toBeVisible()
  })

  test('rating display section is visible', async ({ page }) => {
    await expect(page.locator('text=Rating Display')).toBeVisible()
    await expect(page.locator('text=Rating order')).toBeVisible()
  })

  test('rating limit input defaults to 3', async ({ page }) => {
    const limitInput = page.locator('#ratings-limit-global')
    await expect(limitInput).toBeVisible()
    await expect(limitInput).toHaveValue('3')
  })

  test('all 10 rating sources are listed in order', async ({ page }) => {
    const ratingSection = page.locator('text=Rating order').locator('..')
    for (const label of ['IMDb', 'TMDB', 'Rotten Tomatoes (Critics)', 'Rotten Tomatoes (Audience)', 'Metacritic', 'Trakt', 'Letterboxd', 'MyAnimeList', 'MDBList', 'Roger Ebert']) {
      await expect(ratingSection.locator(`text=${label}`)).toBeVisible()
    }
  })

  test('rating settings persist after auto-save and reload', async ({ page }) => {
    // Change limit to a non-default value
    const limitInput = page.locator('#ratings-limit-global')
    await limitInput.fill('5')

    // Wait for auto-save confirmation
    await expect(page.locator('text=Saved')).toBeVisible({ timeout: 5000 })

    // Reload
    await page.reload()
    await expect(page.locator('h1')).toContainText('Settings')

    // Limit should be preserved
    await expect(page.locator('#ratings-limit-global')).toHaveValue('5')
  })

  test('exclude ratings section is visible with a checkbox per source', async ({ page }) => {
    await expect(page.locator('text=Exclude ratings')).toBeVisible()
    await expect(page.getByTestId('exclude-rt-checkbox')).toBeVisible()
    await expect(page.getByTestId('exclude-mal-checkbox')).toBeVisible()
  })

  test('excluding a rating persists after auto-save and reload', async ({ page }) => {
    const excludeRt = page.getByTestId('exclude-rt-checkbox')
    await expect(excludeRt).not.toBeChecked()

    // Exclude Rotten Tomatoes (Critics) — the scenario from issue #17
    await excludeRt.check()
    await expect(page.locator('text=Saved')).toBeVisible({ timeout: 5000 })

    await page.reload()
    await expect(page.locator('h1')).toContainText('Settings')

    // Exclusion should be preserved across reload
    await expect(page.getByTestId('exclude-rt-checkbox')).toBeChecked()
  })

  test('sidebar navigation to settings works', async ({ page }) => {
    // Navigate away
    await page.click('text=Dashboard')
    await expect(page).toHaveURL(/\/admin$/)

    // Navigate back via sidebar
    await page.click('text=Settings')
    await expect(page).toHaveURL(/\/admin\/settings/)
  })

  test('preview section is visible', async ({ page }) => {
    const previewImg = page.locator('img[alt="Poster preview"]')
    await expect(previewImg).toBeVisible()
  })

  test('preview image loads as valid image', async ({ page }) => {
    const previewImg = page.locator('img[alt="Poster preview"]')
    await expect(previewImg).toBeVisible()

    // Preview uses blob URLs fetched with auth — wait for image to load
    await expect(previewImg).toHaveJSProperty('complete', true)
    const naturalWidth = await previewImg.evaluate((img: HTMLImageElement) => img.naturalWidth)
    expect(naturalWidth).toBeGreaterThan(0)
  })

  test('preview image uses blob URL (fetched with auth)', async ({ page }) => {
    const previewImg = page.locator('img[alt="Poster preview"]')
    await expect(previewImg).toBeVisible()

    const src = await previewImg.getAttribute('src')
    expect(src).toContain('blob:')
  })

  test('preview updates when ratings limit changes', async ({ page }) => {
    const previewImg = page.locator('img[alt="Poster preview"]')
    await expect(previewImg).toBeVisible()

    const initialSrc = await previewImg.getAttribute('src')

    // Read the current limit and change to a different value
    const limitInput = page.locator('#ratings-limit-global')
    const currentValue = await limitInput.inputValue()
    const newValue = currentValue === '7' ? '2' : '7'
    await limitInput.fill(newValue)

    // Wait for network
    await page.waitForTimeout(1000)

    const newSrc = await previewImg.getAttribute('src')
    // Blob URL should change when preview is re-fetched
    expect(newSrc).not.toBe(initialSrc)
  })

  test('poster position dropdown is visible with default', async ({ page }) => {
    // Exact match: the overlay-badge "Quality badge position" / "Main-language
    // badge position" labels also contain "Badge position".
    await expect(page.getByText('Badge position', { exact: true })).toBeVisible()
    const posSelect = page.getByTestId('poster-position-select')
    await expect(posSelect).toContainText('Bottom Center')
  })

  test('poster position persists after change and reload', async ({ page }) => {
    const posSelect = page.getByTestId('poster-position-select')
    await selectOption(page, posSelect, 'Left')

    await expect(page.locator('text=Saved')).toBeVisible({ timeout: 5000 })

    await page.reload()
    await expect(page.locator('h1')).toContainText('Settings')
    await expect(page.getByTestId('poster-position-select')).toContainText('Left')
  })

  test('badge direction dropdown is visible with default', async ({ page }) => {
    const dirSelect = page.getByTestId('poster-badge-direction-select')
    await expect(dirSelect).toBeVisible()
    await expect(dirSelect).toContainText('Default')
  })

  test('badge direction persists after change and reload', async ({ page }) => {
    const dirSelect = page.getByTestId('poster-badge-direction-select')
    await selectOption(page, dirSelect, 'Vertical')

    await expect(page.locator('text=Saved')).toBeVisible({ timeout: 5000 })

    await page.reload()
    await expect(page.locator('h1')).toContainText('Settings')
    await expect(page.getByTestId('poster-badge-direction-select')).toContainText('Vertical')
  })

  test('new poster position options are available', async ({ page }) => {
    const posSelect = page.getByTestId('poster-position-select')
    await posSelect.click()

    for (const label of ['Top Left', 'Top Right', 'Bottom Left', 'Bottom Right']) {
      await expect(page.getByRole('option', { name: label })).toBeVisible()
    }

    // Close the dropdown by pressing Escape
    await page.keyboard.press('Escape')
  })

  test('label style dropdowns are visible with default official', async ({ page }) => {
    // Check poster, logo, and backdrop label style selects
    for (const testId of ['poster-label-style-select', 'logo-label-style-select', 'backdrop-label-style-select']) {
      const select = page.getByTestId(testId)
      await expect(select).toBeVisible()
      await expect(select).toContainText('Official')
    }
  })

  test('label style persists after change and reload', async ({ page }) => {
    // Change poster label style to Text
    const labelSelect = page.getByTestId('poster-label-style-select')
    await selectOption(page, labelSelect, 'Text')

    await expect(page.locator('text=Saved')).toBeVisible({ timeout: 5000 })

    await page.reload()
    await expect(page.locator('h1')).toContainText('Settings')

    await expect(page.getByTestId('poster-label-style-select')).toContainText('Text')
  })
})
