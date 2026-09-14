import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { createRouter, createMemoryHistory } from 'vue-router'
import ImageListView from '@/components/ImageListView.vue'

const sampleResponse = {
  items: [
    {
      cache_key: 'imdb/tt0111161',
      release_date: '1994-09-23',
      created_at: 1710000000,
      updated_at: 1710100000,
    },
    {
      cache_key: 'tmdb/550',
      release_date: '1999-10-15',
      created_at: 1710000000,
      updated_at: 1710100000,
    },
  ],
  total: 2,
  page: 1,
  page_size: 50,
}

function makeMocks() {
  return {
    listFn: vi.fn(),
    imageFn: vi.fn(),
    fetchFn: vi.fn(),
    deleteFn: vi.fn(),
    clearAllFn: vi.fn(),
  }
}

function mountView(mocks: ReturnType<typeof makeMocks>, kind: 'poster' | 'logo' | 'backdrop' = 'poster') {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/', component: { template: '<div />' } }],
  })
  return mount(ImageListView, {
    props: {
      kind,
      listFn: mocks.listFn,
      imageFn: mocks.imageFn,
      fetchFn: mocks.fetchFn,
      deleteFn: mocks.deleteFn,
      clearAllFn: mocks.clearAllFn,
    },
    global: {
      plugins: [createPinia(), router, [VueQueryPlugin, { queryClient }]],
      stubs: {
        Button: {
          template: '<button @click="$emit(\'click\', $event)" :disabled="disabled"><slot /></button>',
          props: ['disabled', 'variant', 'size', 'type'],
          emits: ['click'],
        },
        Skeleton: { template: '<div data-testid="skeleton" />' },
        Table: { template: '<table><slot /></table>' },
        TableHeader: { template: '<thead><slot /></thead>' },
        TableBody: { template: '<tbody><slot /></tbody>' },
        TableRow: { template: '<tr><slot /></tr>' },
        TableHead: { template: '<th><slot /></th>' },
        TableCell: { template: '<td><slot /></td>' },
        RefreshButton: {
          template: '<button @click="$emit(\'refresh\')">Refresh</button>',
          props: ['fetching'],
          emits: ['refresh'],
        },
        Dialog: { template: '<div v-if="open"><slot /></div>', props: ['open'] },
        DialogContent: { template: '<div><slot /></div>' },
        DialogHeader: { template: '<div><slot /></div>' },
        DialogTitle: { template: '<div><slot /></div>' },
        Input: {
          template: '<input :value="modelValue" @input="$emit(\'update:modelValue\', $event.target.value)" />',
          props: ['modelValue'],
          emits: ['update:modelValue'],
        },
        Download: { template: '<span />' },
        Loader2: { template: '<span />' },
        Eye: { template: '<span />' },
        Trash2: { template: '<span />' },
      },
    },
  })
}

describe('ImageListView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('shows skeletons while loading', () => {
    const mocks = makeMocks()
    mocks.listFn.mockReturnValue(new Promise(() => {}))
    const wrapper = mountView(mocks)
    expect(wrapper.findAll('[data-testid="skeleton"]').length).toBeGreaterThan(0)
  })

  it('renders list with parsed cache keys', async () => {
    const mocks = makeMocks()
    mocks.listFn.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(sampleResponse),
    })

    const wrapper = mountView(mocks)
    await flushPromises()

    expect(wrapper.text()).toContain('imdb')
    expect(wrapper.text()).toContain('tt0111161')
    expect(wrapper.text()).toContain('tmdb')
    expect(wrapper.text()).toContain('550')
    expect(wrapper.text()).toContain('1994-09-23')
  })

  it('shows empty state when no items', async () => {
    const mocks = makeMocks()
    mocks.listFn.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ items: [], total: 0, page: 1, page_size: 50 }),
    })

    const wrapper = mountView(mocks)
    await flushPromises()

    expect(wrapper.text()).toContain('No posters cached yet.')
  })

  it('shows total count and pagination info', async () => {
    const mocks = makeMocks()
    mocks.listFn.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(sampleResponse),
    })

    const wrapper = mountView(mocks)
    await flushPromises()

    expect(wrapper.text()).toContain('2 posters total')
    expect(wrapper.text()).toContain('Page 1 of 1')
  })

  it('has a refresh button', async () => {
    const mocks = makeMocks()
    mocks.listFn.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(sampleResponse),
    })

    const wrapper = mountView(mocks)
    await flushPromises()

    const refreshButton = wrapper.findAll('button').find((b) => b.text().includes('Refresh'))
    expect(refreshButton).toBeDefined()
  })

  it('has a fetch button', async () => {
    const mocks = makeMocks()
    mocks.listFn.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(sampleResponse),
    })

    const wrapper = mountView(mocks)
    await flushPromises()

    const fetchButton = wrapper.findAll('button').find((b) => b.text().includes('Fetch'))
    expect(fetchButton).toBeDefined()
  })

  it('opens fetch modal when fetch button is clicked', async () => {
    const mocks = makeMocks()
    mocks.listFn.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(sampleResponse),
    })

    const wrapper = mountView(mocks)
    await flushPromises()

    // Modal content should not be visible initially
    expect(wrapper.text()).not.toContain('Fetch Poster')

    const fetchButton = wrapper.findAll('button').find((b) => b.text().includes('Fetch'))
    await fetchButton!.trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('Fetch Poster')
    expect(wrapper.text()).toContain('ID Type')
    expect(wrapper.text()).toContain('ID Value')
  })

  it('calls fetchFn on form submit', async () => {
    const mocks = makeMocks()
    mocks.listFn.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(sampleResponse),
    })
    mocks.fetchFn.mockResolvedValue({
      ok: true,
      blob: () => Promise.resolve(new Blob()),
    })
    mocks.imageFn.mockResolvedValue({
      ok: true,
      blob: () => Promise.resolve(new Blob()),
    })

    const wrapper = mountView(mocks)
    await flushPromises()

    // Open modal
    const fetchButton = wrapper.findAll('button').find((b) => b.text().includes('Fetch'))
    await fetchButton!.trigger('click')
    await flushPromises()

    // Fill in ID value
    const input = wrapper.find('input')
    await input.setValue('tt0111161')
    await flushPromises()

    // Submit form
    const form = wrapper.find('form')
    await form.trigger('submit')
    await flushPromises()

    expect(mocks.fetchFn).toHaveBeenCalledWith('imdb', 'tt0111161')
  })

  async function openPurgeDialog(mocks: ReturnType<typeof makeMocks>) {
    mocks.listFn.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({
        items: [{ cache_key: 'imdb/tt0111161_t_de@imc', release_date: null, created_at: 1710000000, updated_at: 1710000000 }],
        total: 1,
        page: 1,
        page_size: 50,
      }),
    })
    mocks.deleteFn.mockResolvedValue({ ok: true, json: () => Promise.resolve({ ok: true }) })

    const wrapper = mountView(mocks)
    await flushPromises()

    // Confirm dialog is not shown until the row's purge button is clicked.
    expect(wrapper.text()).not.toContain('Purge poster')

    const purgeButton = wrapper.findAll('button').find((b) => b.attributes('aria-label') === 'Purge poster')
    expect(purgeButton).toBeDefined()
    await purgeButton!.trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('Purge poster')
    return wrapper
  }

  it('purges the whole title via the "Entire title" button (bare title id)', async () => {
    const mocks = makeMocks()
    const wrapper = await openPurgeDialog(mocks)

    const titleButton = wrapper.findAll('button').find((b) => b.text().trim() === 'Entire title')
    await titleButton!.trigger('click')
    await flushPromises()

    // The full cache value carries a variant + ratings suffix; the title purge
    // targets the bare title id.
    expect(mocks.deleteFn).toHaveBeenCalledWith('imdb', 'tt0111161', 'title')
  })

  it('purges a single variant via the "This variant" button (full cache value)', async () => {
    const mocks = makeMocks()
    const wrapper = await openPurgeDialog(mocks)

    const variantButton = wrapper.findAll('button').find((b) => b.text().trim() === 'This variant')
    await variantButton!.trigger('click')
    await flushPromises()

    expect(mocks.deleteFn).toHaveBeenCalledWith('imdb', 'tt0111161_t_de@imc', 'variant')
  })

  it('clears every image of the kind via the header "Clear" button', async () => {
    const mocks = makeMocks()
    mocks.listFn.mockResolvedValue({ ok: true, json: () => Promise.resolve(sampleResponse) })
    mocks.clearAllFn.mockResolvedValue({ ok: true, json: () => Promise.resolve({ ok: true, meta_deleted: 2 }) })

    const wrapper = mountView(mocks)
    await flushPromises()

    // The header button opens the confirm dialog (which adds a second "Clear posters" button).
    const trigger = wrapper.findAll('button').find((b) => b.text().trim() === 'Clear posters')
    expect(trigger).toBeDefined()
    await trigger!.trigger('click')
    await flushPromises()

    const confirmButtons = wrapper.findAll('button').filter((b) => b.text().trim() === 'Clear posters')
    expect(confirmButtons.length).toBe(2)
    await confirmButtons[confirmButtons.length - 1]!.trigger('click')
    await flushPromises()

    expect(mocks.clearAllFn).toHaveBeenCalledTimes(1)
    expect(wrapper.text()).toContain('Cleared 2 cached posters')
  })

  it('uses the singular noun when one image is cleared', async () => {
    const mocks = makeMocks()
    mocks.listFn.mockResolvedValue({ ok: true, json: () => Promise.resolve(sampleResponse) })
    mocks.clearAllFn.mockResolvedValue({ ok: true, json: () => Promise.resolve({ ok: true, meta_deleted: 1 }) })

    const wrapper = mountView(mocks)
    await flushPromises()
    await wrapper.findAll('button').find((b) => b.text().trim() === 'Clear posters')!.trigger('click')
    await flushPromises()
    const confirm = wrapper.findAll('button').filter((b) => b.text().trim() === 'Clear posters')
    await confirm[confirm.length - 1]!.trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('Cleared 1 cached poster.')
  })

  it('shows an error and keeps the dialog open when clear fails', async () => {
    const mocks = makeMocks()
    mocks.listFn.mockResolvedValue({ ok: true, json: () => Promise.resolve(sampleResponse) })
    mocks.clearAllFn.mockResolvedValue({ ok: false, status: 500, text: () => Promise.resolve(JSON.stringify({ error: 'boom' })) })

    const wrapper = mountView(mocks)
    await flushPromises()
    await wrapper.findAll('button').find((b) => b.text().trim() === 'Clear posters')!.trigger('click')
    await flushPromises()
    const confirm = wrapper.findAll('button').filter((b) => b.text().trim() === 'Clear posters')
    await confirm[confirm.length - 1]!.trigger('click')
    await flushPromises()

    expect(mocks.clearAllFn).toHaveBeenCalledTimes(1)
    expect(wrapper.text()).toContain('boom')
    // Dialog stays open (the confirm button is still present).
    expect(wrapper.findAll('button').filter((b) => b.text().trim() === 'Clear posters').length).toBe(2)
  })
})
