import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import DashboardView from '@/views/DashboardView.vue'

const mockAdminApi = vi.hoisted(() => ({
  getStats: vi.fn(),
}))

vi.mock('@/lib/api', () => ({
  adminApi: mockAdminApi,
}))

const sampleStats = {
  total_images: 42,
  total_api_keys: 3,
  mem_cache_entries: 100,
  id_cache_entries: 50,
  ratings_cache_entries: 25,
  image_mem_cache_mb: 128,
}

function mountView() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  return mount(DashboardView, {
    global: {
      plugins: [createPinia(), [VueQueryPlugin, { queryClient }]],
      stubs: {
        Button: {
          template: '<button @click="$emit(\'click\')" :disabled="disabled"><slot /></button>',
          props: ['disabled', 'variant', 'size'],
        },
        Card: { template: '<div><slot /></div>' },
        CardHeader: { template: '<div><slot /></div>' },
        CardTitle: { template: '<div><slot /></div>' },
        CardContent: { template: '<div><slot /></div>' },
        Skeleton: { template: '<div data-testid="skeleton" />' },
        RefreshCw: { template: '<span />' },
        ClearCacheButton: {
          template: '<button @click="$emit(\'cleared\', \'Cache cleared — removed 42 cached images.\')">Clear cache</button>',
          emits: ['cleared'],
        },
      },
    },
  })
}

describe('DashboardView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('shows skeletons while loading', () => {
    mockAdminApi.getStats.mockReturnValue(new Promise(() => {})) // never resolves
    const wrapper = mountView()
    expect(wrapper.findAll('[data-testid="skeleton"]').length).toBeGreaterThan(0)
  })

  it('renders stats from API response', async () => {
    mockAdminApi.getStats.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(sampleStats),
    })

    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.text()).toContain('42')
    expect(wrapper.text()).toContain('3')
    expect(wrapper.text()).toContain('100')
    expect(wrapper.text()).toContain('128')
  })

  it('has a refresh button', async () => {
    mockAdminApi.getStats.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(sampleStats),
    })

    const wrapper = mountView()
    await flushPromises()

    const refreshButton = wrapper.findAll('button').find((b) => b.text().includes('Refresh'))
    expect(refreshButton).toBeDefined()
  })

  it('refresh button triggers refetch', async () => {
    mockAdminApi.getStats.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(sampleStats),
    })

    const wrapper = mountView()
    await flushPromises()

    mockAdminApi.getStats.mockClear()
    mockAdminApi.getStats.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ ...sampleStats, total_images: 99 }),
    })

    const refreshButton = wrapper.findAll('button').find((b) => b.text().includes('Refresh'))
    await refreshButton!.trigger('click')
    await flushPromises()

    expect(mockAdminApi.getStats).toHaveBeenCalled()
    expect(wrapper.text()).toContain('99')
  })

  it('shows a message and refetches stats when the cache is cleared', async () => {
    mockAdminApi.getStats.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(sampleStats),
    })

    const wrapper = mountView()
    await flushPromises()

    mockAdminApi.getStats.mockClear()

    // ClearCacheButton owns the confirm flow; here it emits `cleared` on click.
    const clearButton = wrapper.findAll('button').find((b) => b.text().includes('Clear cache'))
    expect(clearButton).toBeDefined()
    await clearButton!.trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('Cache cleared')
    expect(mockAdminApi.getStats).toHaveBeenCalled() // refetched after clear
  })
})
