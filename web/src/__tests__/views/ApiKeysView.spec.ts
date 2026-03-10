import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import ApiKeysView from '@/views/ApiKeysView.vue'

const mockKeysApi = vi.hoisted(() => ({
  list: vi.fn(),
  create: vi.fn(),
  delete: vi.fn(),
  getSettings: vi.fn(),
  updateSettings: vi.fn(),
  deleteSettings: vi.fn(),
}))

vi.mock('@/lib/api', () => ({
  keysApi: mockKeysApi,
}))

const sampleKeys = [
  {
    id: 1,
    name: 'jellyfin-prod',
    key_prefix: 'opdb_abc',
    created_at: '2026-01-01',
    last_used_at: null,
  },
  {
    id: 2,
    name: 'plex-dev',
    key_prefix: 'opdb_xyz',
    created_at: '2026-02-01',
    last_used_at: '2026-03-01',
  },
]

function mountView() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  return mount(ApiKeysView, {
    global: {
      plugins: [createPinia(), [VueQueryPlugin, { queryClient }]],
      stubs: {
        Button: {
          template: '<button @click="$emit(\'click\')"><slot /></button>',
          props: ['disabled', 'variant', 'size'],
        },
        Input: {
          template: '<input :value="modelValue" @input="$emit(\'update:modelValue\', $event.target.value)" />',
          props: ['modelValue', 'type', 'placeholder', 'required'],
        },
      },
    },
  })
}

describe('ApiKeysView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    mockKeysApi.list.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve([]),
    })
  })

  it('renders key list from mocked API response', async () => {
    mockKeysApi.list.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(sampleKeys),
    })

    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.text()).toContain('jellyfin-prod')
    expect(wrapper.text()).toContain('plex-dev')
    expect(wrapper.text()).toContain('opdb_abc')
    expect(wrapper.text()).toContain('opdb_xyz')
  })

  it('shows "No API keys yet." when empty', async () => {
    mockKeysApi.list.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve([]),
    })

    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.text()).toContain('No API keys yet.')
  })

  it('has a refresh button that triggers refetch', async () => {
    mockKeysApi.list.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(sampleKeys),
    })

    const wrapper = mountView()
    await flushPromises()

    mockKeysApi.list.mockClear()
    mockKeysApi.list.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve([sampleKeys[0]]),
    })

    const refreshButton = wrapper.findAll('button').find((b) => b.text().includes('Refresh'))
    expect(refreshButton).toBeDefined()

    await refreshButton!.trigger('click')
    await flushPromises()

    expect(mockKeysApi.list).toHaveBeenCalled()
  })

  it('shows settings button for each key', async () => {
    mockKeysApi.list.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(sampleKeys),
    })

    const wrapper = mountView()
    await flushPromises()

    // Each key should have a settings icon button
    const settingsButtons = wrapper.findAll('button').filter((b) => {
      // The Settings button contains the Settings lucide icon
      return b.find('svg') !== undefined && !b.text().includes('Delete') && !b.text().includes('Create') && !b.text().includes('Refresh')
    })
    // At minimum we should have some non-delete, non-create buttons (the settings gear buttons)
    expect(settingsButtons.length).toBeGreaterThanOrEqual(2)
  })
})
