import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import KeySettingsView from '@/views/KeySettingsView.vue'

const mockRouter = {
  push: vi.fn(),
  replace: vi.fn(),
}

const mockAuthStore = {
  apiKey: 'test-key',
  apiKeyInfo: null as { name: string; key_prefix: string } | null,
  logoutApiKey: vi.fn(),
}

vi.mock('vue-router', () => ({
  useRouter: () => mockRouter,
}))

vi.mock('@/stores/auth', () => ({
  useAuthStore: () => mockAuthStore,
}))

const mockSelfApi = vi.hoisted(() => ({
  getInfo: vi.fn(),
  getSettings: vi.fn(),
  updateSettings: vi.fn(),
  resetSettings: vi.fn(),
}))

vi.mock('@/lib/api', () => ({
  selfApi: mockSelfApi,
}))

function mountView() {
  return mount(KeySettingsView, {
    global: {
      plugins: [createPinia()],
      stubs: {
        Button: {
          template: '<button @click="$emit(\'click\')"><slot /></button>',
          props: ['disabled', 'variant', 'size'],
        },
        Input: {
          template:
            '<input :value="modelValue" @input="$emit(\'update:modelValue\', $event.target.value)" />',
          props: ['modelValue', 'type', 'placeholder'],
        },
        RenderSettingsForm: {
          template: '<div data-testid="settings-form">RenderSettingsForm</div>',
          props: ['settings', 'uid', 'loadSettings', 'saveSettings', 'resetSettings'],
        },
      },
    },
  })
}

const sampleSettings = {
  poster_source: 't',
  fanart_lang: 'en',
  fanart_textless: false,
  fanart_available: true,
  ratings_limit: 3,
  ratings_order: 'mal,imdb,lb,rt,rta,mc,tmdb,trakt',
  is_default: true,
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

describe('KeySettingsView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    mockAuthStore.apiKey = 'test-key'
    mockAuthStore.apiKeyInfo = null
  })

  it('loads and displays key info and settings form', async () => {
    mockSelfApi.getInfo.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ name: 'my-key', key_prefix: 'abcd1234' }),
    })
    mockSelfApi.getSettings.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(sampleSettings),
    })

    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.text()).toContain('my-key')
    expect(wrapper.text()).toContain('abcd1234')
    expect(wrapper.find('[data-testid="settings-form"]').exists()).toBe(true)
  })

  it('redirects to login when API returns error', async () => {
    mockSelfApi.getInfo.mockResolvedValue({ ok: false })
    mockSelfApi.getSettings.mockResolvedValue({ ok: false })

    mountView()
    await flushPromises()

    expect(mockAuthStore.logoutApiKey).toHaveBeenCalled()
    expect(mockRouter.replace).toHaveBeenCalledWith('/login')
  })

  it('logout button clears session and navigates to login', async () => {
    mockSelfApi.getInfo.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ name: 'k', key_prefix: 'ab' }),
    })
    mockSelfApi.getSettings.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(sampleSettings),
    })

    const wrapper = mountView()
    await flushPromises()

    const logoutBtn = wrapper.findAll('button').find((b) => b.text().includes('Logout'))
    expect(logoutBtn).toBeDefined()
    await logoutBtn!.trigger('click')

    expect(mockAuthStore.logoutApiKey).toHaveBeenCalled()
    expect(mockRouter.push).toHaveBeenCalledWith('/login')
  })

  it('shows heading "Poster Settings"', async () => {
    mockSelfApi.getInfo.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ name: 'k', key_prefix: 'ab' }),
    })
    mockSelfApi.getSettings.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(sampleSettings),
    })

    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.find('h1').text()).toContain('Image Settings')
  })
})
