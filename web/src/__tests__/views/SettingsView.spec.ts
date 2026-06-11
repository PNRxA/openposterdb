import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import SettingsView from '@/views/SettingsView.vue'
import { shadcnStubs } from '@/__tests__/stubs'

const mockAdminApi = vi.hoisted(() => ({
  getSettings: vi.fn(),
  updateSettings: vi.fn(),
  previewPoster: vi.fn().mockResolvedValue({ ok: true, blob: () => Promise.resolve(new Blob()) }),
  previewLogo: vi.fn().mockResolvedValue({ ok: true, blob: () => Promise.resolve(new Blob()) }),
  previewBackdrop: vi.fn().mockResolvedValue({ ok: true, blob: () => Promise.resolve(new Blob()) }),
  previewEpisode: vi.fn().mockResolvedValue({ ok: true, blob: () => Promise.resolve(new Blob()) }),
  previewSeason: vi.fn().mockResolvedValue({ ok: true, blob: () => Promise.resolve(new Blob()) }),
}))

vi.mock('@/lib/api', () => ({
  adminApi: mockAdminApi,
}))

const defaultSettings = {
  image_source: 't',
  lang: 'en',
  textless: false,
  fanart_available: true,
  ratings_limit: 3,
  ratings_order: 'mal,imdb,lb,rt,rta,mc,tmdb,trakt',
  ratings_exclude: '',
  free_api_key_enabled: false,
  poster_position: 'bc',
  logo_ratings_limit: 3,
  backdrop_ratings_limit: 3,
  poster_badge_style: 'h',
  logo_badge_style: 'h',
  backdrop_badge_style: 'v',
  poster_label_style: 't',
  logo_label_style: 't',
  backdrop_label_style: 't',
  poster_badge_direction: 'd',
  poster_badge_split: false,
  backdrop_position: 'tr',
  backdrop_badge_direction: 'v',
  backdrop_edge_inset_x: 0,
  backdrop_edge_inset_y: 0,
  episode_ratings_limit: 1,
  episode_badge_style: 'v',
  episode_label_style: 'o',
  episode_badge_size: 'l',
  episode_position: 'tr',
  episode_badge_direction: 'v',
  episode_blur: false,
  season_ratings_limit: 3,
  season_badge_style: 'd',
  season_label_style: 'o',
  season_badge_size: 'm',
  season_position: 'bc',
  season_badge_direction: 'd',
}

function mountView() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  return mount(SettingsView, {
    global: {
      plugins: [createPinia(), [VueQueryPlugin, { queryClient }]],
      stubs: {
        ...shadcnStubs,
        Input: {
          template:
            '<input :value="modelValue" @input="$emit(\'update:modelValue\', $event.target.value)" />',
          props: ['modelValue', 'type', 'placeholder'],
        },
        RefreshButton: {
          template: '<button @click="$emit(\'refresh\')">Refresh</button>',
          props: ['fetching'],
        },
      },
    },
  })
}

describe('SettingsView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    mockAdminApi.getSettings.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(defaultSettings),
    })
  })

  it('renders settings heading', async () => {
    const wrapper = mountView()
    await flushPromises()
    expect(wrapper.text()).toContain('Settings')
    expect(wrapper.text()).toContain('Global Image Settings')
  })

  it('loads and displays current settings with fanart enabled', async () => {
    mockAdminApi.getSettings.mockResolvedValue({
      ok: true,
      json: () =>
        Promise.resolve({
          ...defaultSettings,
          image_source: 'f',
          lang: 'de',
          textless: true,
        }),
    })

    const wrapper = mountView()
    await flushPromises()

    const fanartCheckbox = wrapper.find('[data-testid="fanart-checkbox"]')
    expect((fanartCheckbox.element as HTMLInputElement).checked).toBe(true)
    const textlessCheckbox = wrapper.find('[data-testid="textless-checkbox"]')
    expect((textlessCheckbox.element as HTMLInputElement).checked).toBe(true)
  })

  it('shows fanart checkbox when fanart is available', async () => {
    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.find('[data-testid="fanart-checkbox"]').exists()).toBe(true)
  })

  it('hides fanart options when not available', async () => {
    mockAdminApi.getSettings.mockResolvedValue({
      ok: true,
      json: () =>
        Promise.resolve({
          ...defaultSettings,
          fanart_available: false,
        }),
    })

    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.find('[data-testid="fanart-checkbox"]').exists()).toBe(false)
  })

  it('auto-saves when fanart checkbox is toggled', async () => {
    vi.useFakeTimers()
    mockAdminApi.updateSettings.mockResolvedValue({ ok: true })

    const wrapper = mountView()
    await flushPromises()

    // Check fanart to trigger auto-save
    await wrapper.find('[data-testid="fanart-checkbox"]').setValue(true)
    vi.advanceTimersByTime(700)
    await flushPromises()

    expect(mockAdminApi.updateSettings).toHaveBeenCalledWith(
      expect.objectContaining({
        image_source: 'f',
        lang: 'en',
        textless: false,
      }),
    )
    vi.useRealTimers()
  })

  it('shows saved indicator after auto-save', async () => {
    vi.useFakeTimers()
    mockAdminApi.updateSettings.mockResolvedValue({ ok: true })

    const wrapper = mountView()
    await flushPromises()

    await wrapper.find('[data-testid="fanart-checkbox"]').setValue(true)
    vi.advanceTimersByTime(700)
    await flushPromises()

    expect(wrapper.find('.text-green-500').exists()).toBe(true)
    vi.useRealTimers()
  })

  it('shows error message on auto-save failure', async () => {
    vi.useFakeTimers()
    mockAdminApi.updateSettings.mockResolvedValue({
      ok: false,
      json: () => Promise.resolve({ error: 'Invalid language' }),
    })

    const wrapper = mountView()
    await flushPromises()

    await wrapper.find('[data-testid="fanart-checkbox"]').setValue(true)
    vi.advanceTimersByTime(700)
    await flushPromises()

    expect(wrapper.text()).toContain('Invalid language')
    vi.useRealTimers()
  })

  it('includes ratings fields in auto-save payload', async () => {
    vi.useFakeTimers()
    mockAdminApi.getSettings.mockResolvedValue({
      ok: true,
      json: () =>
        Promise.resolve({
          ...defaultSettings,
          ratings_limit: 3,
          ratings_order: 'mal,imdb,trakt,rt,rta,mc,tmdb,lb',
        }),
    })
    mockAdminApi.updateSettings.mockResolvedValue({ ok: true })

    const wrapper = mountView()
    await flushPromises()

    // Toggle fanart to trigger auto-save
    await wrapper.find('[data-testid="fanart-checkbox"]').setValue(true)
    vi.advanceTimersByTime(700)
    await flushPromises()

    expect(mockAdminApi.updateSettings).toHaveBeenCalledWith(
      expect.objectContaining({
        ratings_limit: 3,
        ratings_order: expect.stringContaining('mal'),
      }),
    )
    vi.useRealTimers()
  })

  it('shows generic error on network failure', async () => {
    vi.useFakeTimers()
    mockAdminApi.updateSettings.mockRejectedValue(new Error('Network error'))

    const wrapper = mountView()
    await flushPromises()

    await wrapper.find('[data-testid="fanart-checkbox"]').setValue(true)
    vi.advanceTimersByTime(700)
    await flushPromises()

    expect(wrapper.text()).toContain('Failed to save')
    vi.useRealTimers()
  })

  // --- Free API Key toggle ---

  it('renders Free API Key section', async () => {
    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.text()).toContain('Free API Key')
    expect(wrapper.text()).toContain('t0-free-rpdb')
  })

  it('shows toggle as disabled by default', async () => {
    const wrapper = mountView()
    await flushPromises()

    const toggle = wrapper.find('button[role="switch"]')
    expect(toggle.exists()).toBe(true)
    expect(toggle.attributes('aria-checked')).toBe('false')
    expect(wrapper.text()).toContain('Disabled')
  })

  it('shows toggle as enabled when settings say so', async () => {
    mockAdminApi.getSettings.mockResolvedValue({
      ok: true,
      json: () =>
        Promise.resolve({
          ...defaultSettings,
          free_api_key_enabled: true,
        }),
    })

    const wrapper = mountView()
    await flushPromises()

    const toggle = wrapper.find('button[role="switch"]')
    expect(toggle.attributes('aria-checked')).toBe('true')
    expect(wrapper.text()).toContain('Enabled')
  })

  it('toggles free API key and calls updateSettings', async () => {
    mockAdminApi.updateSettings.mockResolvedValue({ ok: true })

    const wrapper = mountView()
    await flushPromises()

    const toggle = wrapper.find('button[role="switch"]')
    await toggle.trigger('click')
    await flushPromises()

    expect(mockAdminApi.updateSettings).toHaveBeenCalledWith(
      expect.objectContaining({
        free_api_key_enabled: true,
      }),
    )
  })

  it('auto-save does not include free_api_key_enabled', async () => {
    vi.useFakeTimers()
    mockAdminApi.getSettings.mockResolvedValue({
      ok: true,
      json: () =>
        Promise.resolve({
          ...defaultSettings,
          free_api_key_enabled: true,
        }),
    })
    mockAdminApi.updateSettings.mockResolvedValue({ ok: true })

    const wrapper = mountView()
    await flushPromises()

    // Toggle fanart to trigger auto-save
    await wrapper.find('[data-testid="fanart-checkbox"]').setValue(true)
    vi.advanceTimersByTime(700)
    await flushPromises()

    expect(mockAdminApi.updateSettings).toHaveBeenCalledWith(
      expect.not.objectContaining({
        free_api_key_enabled: expect.anything(),
      }),
    )
    vi.useRealTimers()
  })

  it('auto-save payload includes episode settings', async () => {
    vi.useFakeTimers()
    mockAdminApi.getSettings.mockResolvedValue({
      ok: true,
      json: () =>
        Promise.resolve({
          ...defaultSettings,
          episode_ratings_limit: 2,
          episode_badge_style: 'h',
          episode_label_style: 'i',
          episode_badge_size: 's',
          episode_position: 'tl',
          episode_badge_direction: 'h',
          episode_blur: true,
        }),
    })
    mockAdminApi.updateSettings.mockResolvedValue({ ok: true })

    const wrapper = mountView()
    await flushPromises()

    // Toggle fanart to trigger auto-save
    await wrapper.find('[data-testid="fanart-checkbox"]').setValue(true)
    vi.advanceTimersByTime(700)
    await flushPromises()

    expect(mockAdminApi.updateSettings).toHaveBeenCalledWith(
      expect.objectContaining({
        episode_ratings_limit: 2,
        episode_badge_style: 'h',
        episode_label_style: 'i',
        episode_badge_size: 's',
        episode_position: 'tl',
        episode_badge_direction: 'h',
        episode_blur: true,
      }),
    )
    vi.useRealTimers()
  })

  it('auto-save payload includes season settings', async () => {
    vi.useFakeTimers()
    mockAdminApi.getSettings.mockResolvedValue({
      ok: true,
      json: () =>
        Promise.resolve({
          ...defaultSettings,
          season_ratings_limit: 2,
          season_badge_style: 'h',
          season_label_style: 'i',
          season_badge_size: 's',
          season_position: 'tl',
          season_badge_direction: 'h',
        }),
    })
    mockAdminApi.updateSettings.mockResolvedValue({ ok: true })

    const wrapper = mountView()
    await flushPromises()

    // Toggle fanart to trigger auto-save
    await wrapper.find('[data-testid="fanart-checkbox"]').setValue(true)
    vi.advanceTimersByTime(700)
    await flushPromises()

    expect(mockAdminApi.updateSettings).toHaveBeenCalledWith(
      expect.objectContaining({
        season_ratings_limit: 2,
        season_badge_style: 'h',
        season_label_style: 'i',
        season_badge_size: 's',
        season_position: 'tl',
        season_badge_direction: 'h',
      }),
    )
    vi.useRealTimers()
  })

  it('toggleFreeApiKey payload includes season fields', async () => {
    mockAdminApi.updateSettings.mockResolvedValue({ ok: true })

    const wrapper = mountView()
    await flushPromises()

    const toggle = wrapper.find('button[role="switch"]')
    await toggle.trigger('click')
    await flushPromises()

    expect(mockAdminApi.updateSettings).toHaveBeenCalledWith(
      expect.objectContaining({
        season_ratings_limit: 3,
        season_badge_style: 'd',
        season_label_style: 'o',
        season_badge_size: 'm',
        season_position: 'bc',
        season_badge_direction: 'd',
      }),
    )
  })

  it('toggleFreeApiKey payload includes episode fields', async () => {
    mockAdminApi.updateSettings.mockResolvedValue({ ok: true })

    const wrapper = mountView()
    await flushPromises()

    const toggle = wrapper.find('button[role="switch"]')
    await toggle.trigger('click')
    await flushPromises()

    expect(mockAdminApi.updateSettings).toHaveBeenCalledWith(
      expect.objectContaining({
        episode_ratings_limit: 1,
        episode_badge_style: 'v',
        episode_label_style: 'o',
        episode_badge_size: 'l',
        episode_position: 'tr',
        episode_badge_direction: 'v',
        episode_blur: false,
      }),
    )
  })

  it('toggleFreeApiKey payload preserves backdrop position, direction, and edge insets', async () => {
    // Regression guard: the toggle must forward the full settings, not a curated
    // subset. A previous payload omitted these backdrop fields, so flipping the
    // free-key switch silently reset them to their serde defaults on the server.
    mockAdminApi.getSettings.mockResolvedValue({
      ok: true,
      json: () =>
        Promise.resolve({
          ...defaultSettings,
          backdrop_position: 'bl',
          backdrop_badge_direction: 'h',
          backdrop_edge_inset_x: 12,
          backdrop_edge_inset_y: 7,
        }),
    })
    mockAdminApi.updateSettings.mockResolvedValue({ ok: true })

    const wrapper = mountView()
    await flushPromises()

    const toggle = wrapper.find('button[role="switch"]')
    await toggle.trigger('click')
    await flushPromises()

    expect(mockAdminApi.updateSettings).toHaveBeenCalledWith(
      expect.objectContaining({
        free_api_key_enabled: true,
        backdrop_position: 'bl',
        backdrop_badge_direction: 'h',
        backdrop_edge_inset_x: 12,
        backdrop_edge_inset_y: 7,
      }),
    )
  })
})
