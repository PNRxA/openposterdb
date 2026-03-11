import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import PosterSettingsForm from '@/components/PosterSettingsForm.vue'
import type { PosterSettings } from '@/components/PosterSettingsForm.vue'

vi.mock('@/lib/api', () => ({}))

const defaultSettings: PosterSettings = {
  poster_source: 't',
  fanart_lang: 'en',
  fanart_textless: false,
  fanart_available: true,
  ratings_limit: 3,
  ratings_order: 'mal,imdb,lb,rt,rta,mc,tmdb,trakt',
  poster_position: 'bc',
  logo_ratings_limit: 3,
  backdrop_ratings_limit: 3,
  poster_badge_style: 'h',
  logo_badge_style: 'h',
  backdrop_badge_style: 'v',
  poster_label_style: 'i',
  logo_label_style: 'i',
  backdrop_label_style: 'i',
  poster_badge_direction: 'd',
}

function makeFetchPreview() {
  return vi.fn().mockResolvedValue({
    ok: true,
    blob: () => Promise.resolve(new Blob(['fake-jpeg'], { type: 'image/jpeg' })),
  })
}

function mountForm(overrides: Partial<PosterSettings> = {}, fetchPreview = makeFetchPreview()) {
  const settings = { ...defaultSettings, ...overrides }
  return mount(PosterSettingsForm, {
    props: {
      settings,
      loadSettings: vi.fn().mockResolvedValue(settings),
      saveSettings: vi.fn().mockResolvedValue(null),
      fetchPreview,
    },
    global: {
      plugins: [createPinia()],
      stubs: {
        Button: {
          template: '<button :disabled="disabled" @click="$emit(\'click\')"><slot /></button>',
          props: ['disabled', 'variant', 'size'],
        },
      },
    },
  })
}

describe('PosterSettingsForm', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('renders preview section', () => {
    const wrapper = mountForm()
    expect(wrapper.text()).toContain('Poster')
    expect(wrapper.find('img[alt="Poster preview"]').exists()).toBe(true)
  })

  it('calls fetchPreview on mount', async () => {
    const fetchPreview = makeFetchPreview()
    mountForm({}, fetchPreview)
    await flushPromises()

    expect(fetchPreview).toHaveBeenCalledWith(3, 'mal,imdb,lb,rt,rta,mc,tmdb,trakt', 'bc', 'h', 'i', 'd')
  })

  it('calls fetchPreview with correct params for custom settings', async () => {
    const fetchPreview = makeFetchPreview()
    mountForm({ ratings_limit: 5, ratings_order: 'imdb,rt,tmdb' }, fetchPreview)
    await flushPromises()

    expect(fetchPreview).toHaveBeenCalledWith(5, expect.stringContaining('imdb'), expect.any(String), expect.any(String), expect.any(String), expect.any(String))
  })

  it('sets preview src from blob after fetch', async () => {
    const wrapper = mountForm()
    await flushPromises()

    const img = wrapper.find('img[alt="Poster preview"]')
    const src = img.attributes('src')
    expect(src).toBeTruthy()
    expect(src).toContain('blob:')
  })

  it('updates preview after debounce when ratings_limit changes', async () => {
    const fetchPreview = makeFetchPreview()
    const wrapper = mountForm({}, fetchPreview)
    await flushPromises()
    fetchPreview.mockClear()

    // Change the limit
    const limitInput = wrapper.find('input[type="number"]')
    await limitInput.setValue(5)
    await flushPromises()

    // Before debounce fires, fetchPreview should not have been called again
    expect(fetchPreview).not.toHaveBeenCalled()

    // Advance past debounce timer
    vi.advanceTimersByTime(600)
    await flushPromises()

    expect(fetchPreview).toHaveBeenCalledWith(5, expect.any(String), expect.any(String), expect.any(String), expect.any(String), expect.any(String))
  })

  it('debounces rapid changes — only last value triggers fetch', async () => {
    const fetchPreview = makeFetchPreview()
    const wrapper = mountForm({}, fetchPreview)
    await flushPromises()
    fetchPreview.mockClear()

    const limitInput = wrapper.find('input[type="number"]')

    // Rapid changes
    await limitInput.setValue(1)
    vi.advanceTimersByTime(200)
    await limitInput.setValue(4)
    vi.advanceTimersByTime(200)
    await limitInput.setValue(7)

    // Advance past final debounce
    vi.advanceTimersByTime(600)
    await flushPromises()

    // Should only have been called once with the final value
    expect(fetchPreview).toHaveBeenCalledTimes(1)
    expect(fetchPreview).toHaveBeenCalledWith(7, expect.any(String), expect.any(String), expect.any(String), expect.any(String), expect.any(String))
  })

  it('shows loading state while preview loads', async () => {
    // Use a fetch that never resolves to keep loading state
    const fetchPreview = vi.fn().mockReturnValue(new Promise(() => {}))
    const wrapper = mountForm({}, fetchPreview)

    // previewLoading starts true on mount (updatePreview is called)
    const spinner = wrapper.find('.animate-spin')
    expect(spinner.exists()).toBe(true)

    // Image should be hidden while loading (v-show)
    const img = wrapper.find('img[alt="Poster preview"]')
    expect(img.isVisible()).toBe(false)
  })

  it('hides loading spinner and shows image after successful fetch', async () => {
    const wrapper = mountForm()
    await flushPromises()

    // After fetch resolves, trigger image load
    const img = wrapper.find('img[alt="Poster preview"]')
    await img.trigger('load')
    await flushPromises()

    expect(wrapper.find('.animate-spin').exists()).toBe(false)
    expect(img.isVisible()).toBe(true)
  })

  it('shows error message when preview fetch fails', async () => {
    const fetchPreview = vi.fn().mockResolvedValue({ ok: false })
    const wrapper = mountForm({}, fetchPreview)
    await flushPromises()

    expect(wrapper.text()).toContain('Failed')
  })

  it('shows error message when preview fetch throws', async () => {
    const fetchPreview = vi.fn().mockRejectedValue(new Error('Network error'))
    const wrapper = mountForm({}, fetchPreview)
    await flushPromises()

    expect(wrapper.text()).toContain('Failed')
  })

  it('renders poster position dropdown', () => {
    const wrapper = mountForm()
    const select = wrapper.findAll('select')
    const positionSelect = select.find(s => {
      const options = s.findAll('option')
      return options.some(o => o.attributes('value') === 'l')
    })
    expect(positionSelect).toBeDefined()
  })

  it('calls fetchPreview with posterPosition', async () => {
    const fetchPreview = makeFetchPreview()
    mountForm({ poster_position: 'l' }, fetchPreview)
    await flushPromises()

    expect(fetchPreview).toHaveBeenCalledWith(3, expect.any(String), 'l', 'h', 'i', 'd')
  })

  it('renders badge direction dropdown', () => {
    const wrapper = mountForm()
    const select = wrapper.find('[data-testid="poster-badge-direction-select"]')
    expect(select.exists()).toBe(true)
  })

  it('calls fetchPreview with badge direction', async () => {
    const fetchPreview = makeFetchPreview()
    mountForm({ poster_badge_direction: 'v' }, fetchPreview)
    await flushPromises()

    expect(fetchPreview).toHaveBeenCalledWith(3, expect.any(String), 'bc', 'h', 'i', 'v')
  })
})
