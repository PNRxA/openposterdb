import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import SettingsView from '@/views/SettingsView.vue'

const mockAdminApi = vi.hoisted(() => ({
  getSettings: vi.fn(),
  updateSettings: vi.fn(),
}))

vi.mock('@/lib/api', () => ({
  adminApi: mockAdminApi,
}))

const defaultSettings = {
  poster_source: 'tmdb',
  fanart_lang: 'en',
  fanart_textless: false,
  fanart_available: true,
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

  it('calls updateSettings on save', async () => {
    mockAdminApi.updateSettings.mockResolvedValue({ ok: true })

    const wrapper = mountView()
    await flushPromises()

    const saveBtn = wrapper.findAll('button').find((b) => b.text() === 'Save')!
    await saveBtn.trigger('click')
    await flushPromises()

    expect(mockAdminApi.updateSettings).toHaveBeenCalledWith({
      poster_source: 'tmdb',
      fanart_lang: 'en',
      fanart_textless: false,
    })
  })

  it('shows success message after save', async () => {
    mockAdminApi.updateSettings.mockResolvedValue({ ok: true })

    const wrapper = mountView()
    await flushPromises()

    const saveBtn = wrapper.findAll('button').find((b) => b.text() === 'Save')!
    await saveBtn.trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('Saved')
  })

  it('shows error message on save failure', async () => {
    mockAdminApi.updateSettings.mockResolvedValue({
      ok: false,
      json: () => Promise.resolve({ error: 'Invalid language' }),
    })

    const wrapper = mountView()
    await flushPromises()

    const saveBtn = wrapper.findAll('button').find((b) => b.text() === 'Save')!
    await saveBtn.trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('Invalid language')
  })

  it('shows generic error on network failure', async () => {
    mockAdminApi.updateSettings.mockRejectedValue(new Error('Network error'))

    const wrapper = mountView()
    await flushPromises()

    const saveBtn = wrapper.findAll('button').find((b) => b.text() === 'Save')!
    await saveBtn.trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('Failed to save')
  })
})
