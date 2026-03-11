import { test, expect } from '@playwright/test'

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
      data: { poster_source: 'tmdb' },
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
    const select = page.getByTestId('poster-source-select')
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
    await page.getByTestId('poster-source-select').selectOption('fanart')

    // Now language and textless should appear
    await expect(page.locator('label:has-text("Language")')).toBeVisible()
    await expect(page.locator('label:has-text("Prefer textless")')).toBeVisible()
  })

  test('auto-saves and shows confirmation', async ({ page }) => {
    // Change a setting to trigger auto-save
    await page.getByTestId('poster-source-select').selectOption('fanart')

    // Wait for debounced auto-save + confirmation
    await expect(page.locator('text=Saved')).toBeVisible({ timeout: 5000 })
  })

  test('settings persist after auto-save and reload', async ({ page }) => {
    // Select fanart and configure
    await page.getByTestId('poster-source-select').selectOption('fanart')

    // Wait for auto-save confirmation
    await expect(page.locator('text=Saved')).toBeVisible({ timeout: 5000 })

    // Reload page
    await page.reload()
    await expect(page.locator('h1')).toContainText('Settings')

    // Settings should be preserved
    await expect(page.getByTestId('poster-source-select')).toHaveValue('fanart')
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

  test('all 8 rating sources are listed in order', async ({ page }) => {
    const ratingSection = page.locator('text=Rating order').locator('..')
    for (const label of ['IMDb', 'TMDB', 'Rotten Tomatoes (Critics)', 'Rotten Tomatoes (Audience)', 'Metacritic', 'Trakt', 'Letterboxd', 'MyAnimeList']) {
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

  test('sidebar navigation to settings works', async ({ page }) => {
    // Navigate away
    await page.click('text=Dashboard')
    await expect(page).toHaveURL(/\/$/)

    // Navigate back via sidebar
    await page.click('text=Settings')
    await expect(page).toHaveURL(/\/settings/)
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

    // Wait for debounced update (500ms) + network
    await page.waitForTimeout(1500)

    const newSrc = await previewImg.getAttribute('src')
    // Blob URL should change when preview is re-fetched
    expect(newSrc).not.toBe(initialSrc)
  })

  test('poster position dropdown is visible with default', async ({ page }) => {
    await expect(page.locator('text=Badge position')).toBeVisible()
    const posSelect = page.getByTestId('poster-position-select')
    await expect(posSelect).toHaveValue('bc')
  })

  test('poster position persists after change and reload', async ({ page }) => {
    const posSelect = page.getByTestId('poster-position-select')
    await posSelect.selectOption('l')

    await expect(page.locator('text=Saved')).toBeVisible({ timeout: 5000 })

    await page.reload()
    await expect(page.locator('h1')).toContainText('Settings')
    await expect(page.getByTestId('poster-position-select')).toHaveValue('l')
  })

  test('badge direction dropdown is visible with default', async ({ page }) => {
    const dirSelect = page.getByTestId('poster-badge-direction-select')
    await expect(dirSelect).toBeVisible()
    await expect(dirSelect).toHaveValue('d')
  })

  test('badge direction persists after change and reload', async ({ page }) => {
    const dirSelect = page.getByTestId('poster-badge-direction-select')
    await dirSelect.selectOption('v')

    await expect(page.locator('text=Saved')).toBeVisible({ timeout: 5000 })

    await page.reload()
    await expect(page.locator('h1')).toContainText('Settings')
    await expect(page.getByTestId('poster-badge-direction-select')).toHaveValue('v')
  })

  test('new poster position options are available', async ({ page }) => {
    const posSelect = page.getByTestId('poster-position-select')
    for (const value of ['tl', 'tr', 'bl', 'br']) {
      await expect(posSelect.locator(`option[value="${value}"]`)).toBeAttached()
    }
  })

  test('label style dropdowns are visible with default icon', async ({ page }) => {
    const labelSelects = page.locator('select').filter({ has: page.locator('option[value="i"]') })
    // There should be 3 label style selects (poster, logo, backdrop)
    await expect(labelSelects).toHaveCount(3)

    // All should default to "icon"
    for (const select of await labelSelects.all()) {
      await expect(select).toHaveValue('i')
    }
  })

  test('label style persists after change and reload', async ({ page }) => {
    // Find the first label style select (poster) and change to text
    const labelSelects = page.locator('select').filter({ has: page.locator('option[value="i"]') })
    await labelSelects.first().selectOption('t')

    await expect(page.locator('text=Saved')).toBeVisible({ timeout: 5000 })

    await page.reload()
    await expect(page.locator('h1')).toContainText('Settings')

    const reloadedSelects = page.locator('select').filter({ has: page.locator('option[value="i"]') })
    await expect(reloadedSelects.first()).toHaveValue('t')
  })
})
