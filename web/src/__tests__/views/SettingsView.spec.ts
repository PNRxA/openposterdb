import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import SettingsView from '@/views/SettingsView.vue'

const mockAdminApi = vi.hoisted(() => ({
  getSettings: vi.fn(),
  updateSettings: vi.fn(),
  previewPoster: vi.fn().mockResolvedValue({ ok: true, blob: () => Promise.resolve(new Blob()) }),
  previewLogo: vi.fn().mockResolvedValue({ ok: true, blob: () => Promise.resolve(new Blob()) }),
  previewBackdrop: vi.fn().mockResolvedValue({ ok: true, blob: () => Promise.resolve(new Blob()) }),
}))

vi.mock('@/lib/api', () => ({
  adminApi: mockAdminApi,
}))

const defaultSettings = {
  poster_source: 'tmdb',
  fanart_lang: 'en',
  fanart_textless: false,
  fanart_available: true,
  ratings_limit: 3,
  ratings_order: 'mal,imdb,lb,rt,rta,mc,tmdb,trakt',
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
}

function mountView() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  return mount(SettingsView, {
    global: {
      plugins: [createPinia(), [VueQueryPlugin, { queryClient }]],
      stubs: {
        Button: {
          template: '<button :disabled="disabled" @click="$emit(\'click\')"><slot /></button>',
          props: ['disabled', 'variant', 'size'],
        },
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
    expect(wrapper.text()).toContain('Global Poster Defaults')
  })

  it('loads and displays current settings', async () => {
    mockAdminApi.getSettings.mockResolvedValue({
      ok: true,
      json: () =>
        Promise.resolve({
          ...defaultSettings,
          poster_source: 'fanart',
          fanart_lang: 'de',
          fanart_textless: true,
        }),
    })

    const wrapper = mountView()
    await flushPromises()

    const select = wrapper.find('select')
    expect(select.element.value).toBe('fanart')
  })

  it('shows fanart options only when fanart is selected', async () => {
    const wrapper = mountView()
    await flushPromises()

    // Default is tmdb — language/textless fields should be hidden
    expect(wrapper.text()).not.toContain('Language')
    expect(wrapper.text()).not.toContain('Prefer textless')

    // Switch to fanart
    await wrapper.find('select').setValue('fanart')
    await flushPromises()

    expect(wrapper.text()).toContain('Language')
    expect(wrapper.text()).toContain('Prefer textless')
  })

  it('disables fanart option when not available', async () => {
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

    const fanartOption = wrapper.find('option[value="fanart"]')
    expect(fanartOption.attributes('disabled')).toBeDefined()
    expect(fanartOption.text()).toContain('no API key')
  })

  it('auto-saves when settings change', async () => {
    vi.useFakeTimers()
    mockAdminApi.updateSettings.mockResolvedValue({ ok: true })

    const wrapper = mountView()
    await flushPromises()

    // Change poster source to trigger auto-save
    await wrapper.find('select').setValue('fanart')
    vi.advanceTimersByTime(700)
    await flushPromises()

    expect(mockAdminApi.updateSettings).toHaveBeenCalledWith(
      expect.objectContaining({
        poster_source: 'fanart',
        fanart_lang: 'en',
        fanart_textless: false,
      }),
    )
    vi.useRealTimers()
  })

  it('shows saved indicator after auto-save', async () => {
    vi.useFakeTimers()
    mockAdminApi.updateSettings.mockResolvedValue({ ok: true })

    const wrapper = mountView()
    await flushPromises()

    await wrapper.find('select').setValue('fanart')
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

    await wrapper.find('select').setValue('fanart')
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

    // Change something to trigger auto-save
    await wrapper.find('select').setValue('fanart')
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

    await wrapper.find('select').setValue('fanart')
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

    // Change something to trigger auto-save
    await wrapper.find('select').setValue('fanart')
    vi.advanceTimersByTime(700)
    await flushPromises()

    expect(mockAdminApi.updateSettings).toHaveBeenCalledWith(
      expect.not.objectContaining({
        free_api_key_enabled: expect.anything(),
      }),
    )
    vi.useRealTimers()
  })
})
