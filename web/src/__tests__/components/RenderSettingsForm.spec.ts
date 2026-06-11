import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import RenderSettingsForm from '@/components/RenderSettingsForm.vue'
import type { RenderSettings } from '@/components/RenderSettingsForm.vue'
import { shadcnStubs } from '@/__tests__/stubs'

vi.mock('@/lib/api', () => ({}))

const defaultSettings: RenderSettings = {
  image_source: 't',
  lang: 'en',
  textless: false,
  fanart_available: true,
  ratings_limit: 3,
  ratings_order: 'mal,imdb,lb,rt,rta,mc,tmdb,trakt',
  ratings_exclude: '',
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
  poster_badge_split: false,
  poster_fit: 'native',
  poster_badge_size: 'm',
  logo_badge_size: 'm',
  backdrop_badge_size: 'm',
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
  poster_badge_shape: 'r',
  logo_badge_shape: 'r',
  backdrop_badge_shape: 'r',
  episode_badge_shape: 'r',
  season_badge_shape: 'r',
  poster_badge_background: 'd',
  logo_badge_background: 'd',
  backdrop_badge_background: 'd',
  episode_badge_background: 'd',
  season_badge_background: 'd',
}

function makeFetchPreview() {
  return vi.fn().mockResolvedValue({
    ok: true,
    blob: () => Promise.resolve(new Blob(['fake-jpeg'], { type: 'image/jpeg' })),
  })
}

function mountForm(overrides: Partial<RenderSettings> = {}, fetchPreview = makeFetchPreview()) {
  const settings = { ...defaultSettings, ...overrides }
  return mount(RenderSettingsForm, {
    props: {
      settings,
      loadSettings: vi.fn().mockResolvedValue(settings),
      saveSettings: vi.fn().mockResolvedValue(null),
      fetchPreview,
    },
    global: {
      plugins: [createPinia()],
      stubs: shadcnStubs,
    },
  })
}

describe('RenderSettingsForm', () => {
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

    expect(fetchPreview).toHaveBeenCalledWith(3, 'mal,imdb,lb,rt,rta,mc,tmdb,trakt,mdblist,ebert', 'bc', 'h', 'i', 'd', 'm', '', false, 'r', 'd', 'native')
  })

  it('calls fetchPreview with correct params for custom settings', async () => {
    const fetchPreview = makeFetchPreview()
    mountForm({ ratings_limit: 5, ratings_order: 'imdb,rt,tmdb' }, fetchPreview)
    await flushPromises()

    expect(fetchPreview).toHaveBeenCalledWith(5, expect.stringContaining('imdb'), expect.any(String), expect.any(String), expect.any(String), expect.any(String), expect.any(String), '', false, 'r', 'd', 'native')
  })

  it('sets preview src from blob after fetch', async () => {
    const wrapper = mountForm()
    await flushPromises()

    const img = wrapper.find('img[alt="Poster preview"]')
    const src = img.attributes('src')
    expect(src).toBeTruthy()
    expect(src).toContain('blob:')
  })

  it('updates preview when ratings_limit changes', async () => {
    const fetchPreview = makeFetchPreview()
    const wrapper = mountForm({}, fetchPreview)
    await flushPromises()
    fetchPreview.mockClear()

    // Change the limit
    const limitInput = wrapper.find('input[type="number"]')
    await limitInput.setValue(5)

    // Advance past preview debounce timer
    vi.advanceTimersByTime(500)
    await flushPromises()

    expect(fetchPreview).toHaveBeenCalledWith(5, expect.any(String), expect.any(String), expect.any(String), expect.any(String), expect.any(String), expect.any(String), '', false, 'r', 'd', 'native')
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
    const select = wrapper.find('[data-testid="poster-position-select"]')
    expect(select.exists()).toBe(true)
  })

  it('calls fetchPreview with posterPosition', async () => {
    const fetchPreview = makeFetchPreview()
    mountForm({ poster_position: 'l' }, fetchPreview)
    await flushPromises()

    expect(fetchPreview).toHaveBeenCalledWith(3, expect.any(String), 'l', 'h', 'i', 'd', 'm', '', false, 'r', 'd', 'native')
  })

  it('hides fanart checkbox when fanart_available is false', () => {
    const wrapper = mountForm({ fanart_available: false })
    expect(wrapper.find('[data-testid="fanart-checkbox"]').exists()).toBe(false)
  })

  it('shows fanart checkbox when fanart_available is true', () => {
    const wrapper = mountForm({ fanart_available: true })
    expect(wrapper.find('[data-testid="fanart-checkbox"]').exists()).toBe(true)
  })

  it('checks fanart checkbox when source is fanart', () => {
    const wrapper = mountForm({ image_source: 'f' })
    const checkbox = wrapper.find('[data-testid="fanart-checkbox"]')
    expect((checkbox.element as HTMLInputElement).checked).toBe(true)
  })

  it('language and textless are always enabled regardless of fanart checkbox', () => {
    const wrapper = mountForm({ image_source: 't' })
    expect((wrapper.find('[data-testid="textless-checkbox"]').element as HTMLInputElement).disabled).toBe(false)
    expect((wrapper.find('[data-testid="lang-select"]').element as HTMLInputElement).disabled).toBe(false)
  })

  it('defaults language to en when lang is empty', async () => {
    const saveSettings = vi.fn().mockResolvedValue(null)
    const settings = { ...defaultSettings, image_source: 'f', lang: '' }
    const wrapper = mount(RenderSettingsForm, {
      props: {
        settings,
        loadSettings: vi.fn().mockResolvedValue(settings),
        saveSettings,
        fetchPreview: makeFetchPreview(),
      },
      global: {
        plugins: [createPinia()],
        stubs: shadcnStubs,
      },
    })

    // Trigger auto-save by toggling textless to verify lang defaults to 'en'
    await wrapper.find('[data-testid="textless-checkbox"]').setValue(true)
    await flushPromises()

    expect(saveSettings).toHaveBeenCalledWith(
      expect.objectContaining({ lang: 'en' }),
    )
  })

  it('renders badge direction dropdown', () => {
    const wrapper = mountForm()
    const select = wrapper.find('[data-testid="poster-badge-direction-select"]')
    expect(select.exists()).toBe(true)
  })

  // --- Exclude ratings ---

  it('renders an exclude checkbox for every rating source', () => {
    const wrapper = mountForm()
    for (const key of ['imdb', 'tmdb', 'rt', 'rta', 'mc', 'trakt', 'lb', 'mal', 'mdblist', 'ebert']) {
      expect(wrapper.find(`[data-testid="exclude-${key}-checkbox"]`).exists()).toBe(true)
    }
  })

  it('initializes exclude checkboxes from ratings_exclude', () => {
    const wrapper = mountForm({ ratings_exclude: 'rt' })
    expect((wrapper.find('[data-testid="exclude-rt-checkbox"]').element as HTMLInputElement).checked).toBe(true)
    expect((wrapper.find('[data-testid="exclude-imdb-checkbox"]').element as HTMLInputElement).checked).toBe(false)
  })

  it('toggling an exclude checkbox auto-saves ratings_exclude', async () => {
    const saveSettings = vi.fn().mockResolvedValue(null)
    const settings = { ...defaultSettings, ratings_exclude: '' }
    const wrapper = mount(RenderSettingsForm, {
      props: {
        settings,
        loadSettings: vi.fn().mockResolvedValue(settings),
        saveSettings,
        fetchPreview: makeFetchPreview(),
      },
      global: {
        plugins: [createPinia()],
        stubs: shadcnStubs,
      },
    })

    await wrapper.find('[data-testid="exclude-rt-checkbox"]').setValue(true)
    await flushPromises()

    expect(saveSettings).toHaveBeenCalledWith(
      expect.objectContaining({ ratings_exclude: 'rt' }),
    )
  })

  it('calls fetchPreview with badge direction', async () => {
    const fetchPreview = makeFetchPreview()
    mountForm({ poster_badge_direction: 'v' }, fetchPreview)
    await flushPromises()

    expect(fetchPreview).toHaveBeenCalledWith(3, expect.any(String), 'bc', 'h', 'i', 'v', 'm', '', false, 'r', 'd', 'native')
  })

  // --- Episode preview ---

  it('renders episode section when fetchEpisodePreview is provided', () => {
    const fetchEpisodePreview = makeFetchPreview()
    const settings = { ...defaultSettings }
    const wrapper = mount(RenderSettingsForm, {
      props: {
        settings,
        loadSettings: vi.fn().mockResolvedValue(settings),
        saveSettings: vi.fn().mockResolvedValue(null),
        fetchPreview: makeFetchPreview(),
        fetchEpisodePreview,
      },
      global: {
        plugins: [createPinia()],
        stubs: shadcnStubs,
      },
    })
    expect(wrapper.text()).toContain('Episode')
    expect(wrapper.find('img[alt="Episode preview"]').exists()).toBe(true)
  })

  it('does not render episode section when fetchEpisodePreview is absent', () => {
    const wrapper = mountForm()
    expect(wrapper.find('img[alt="Episode preview"]').exists()).toBe(false)
  })

  it('calls fetchEpisodePreview on mount with episode settings', async () => {
    const fetchEpisodePreview = makeFetchPreview()
    const settings = { ...defaultSettings }
    mount(RenderSettingsForm, {
      props: {
        settings,
        loadSettings: vi.fn().mockResolvedValue(settings),
        saveSettings: vi.fn().mockResolvedValue(null),
        fetchPreview: makeFetchPreview(),
        fetchEpisodePreview,
      },
      global: {
        plugins: [createPinia()],
        stubs: shadcnStubs,
      },
    })
    await flushPromises()

    expect(fetchEpisodePreview).toHaveBeenCalledWith(
      1, // episode_ratings_limit
      expect.any(String), // ratings_order
      'v', // episode_badge_style
      'o', // episode_label_style
      'l', // episode_badge_size
      'tr', // episode_position
      'v', // episode_badge_direction
      false, // episode_blur
      '', // ratings_exclude
      'r', // episode_badge_shape
      'd', // episode_badge_background
    )
  })

  it('renders episode position and blur controls', () => {
    const fetchEpisodePreview = makeFetchPreview()
    const settings = { ...defaultSettings }
    const wrapper = mount(RenderSettingsForm, {
      props: {
        settings,
        loadSettings: vi.fn().mockResolvedValue(settings),
        saveSettings: vi.fn().mockResolvedValue(null),
        fetchPreview: makeFetchPreview(),
        fetchEpisodePreview,
      },
      global: {
        plugins: [createPinia()],
        stubs: shadcnStubs,
      },
    })
    expect(wrapper.find('[data-testid="episode-position-select"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="episode-badge-style-select"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="episode-badge-direction-select"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="episode-blur-checkbox"]').exists()).toBe(true)
  })

  // --- Season preview ---

  it('renders season section when fetchSeasonPreview is provided', () => {
    const fetchSeasonPreview = makeFetchPreview()
    const settings = { ...defaultSettings }
    const wrapper = mount(RenderSettingsForm, {
      props: {
        settings,
        loadSettings: vi.fn().mockResolvedValue(settings),
        saveSettings: vi.fn().mockResolvedValue(null),
        fetchPreview: makeFetchPreview(),
        fetchSeasonPreview,
      },
      global: {
        plugins: [createPinia()],
        stubs: shadcnStubs,
      },
    })
    expect(wrapper.text()).toContain('Season')
    expect(wrapper.find('img[alt="Season preview"]').exists()).toBe(true)
  })

  it('does not render season section when fetchSeasonPreview is absent', () => {
    const wrapper = mountForm()
    expect(wrapper.find('img[alt="Season preview"]').exists()).toBe(false)
  })

  it('calls fetchSeasonPreview on mount with season settings', async () => {
    const fetchSeasonPreview = makeFetchPreview()
    const settings = { ...defaultSettings }
    mount(RenderSettingsForm, {
      props: {
        settings,
        loadSettings: vi.fn().mockResolvedValue(settings),
        saveSettings: vi.fn().mockResolvedValue(null),
        fetchPreview: makeFetchPreview(),
        fetchSeasonPreview,
      },
      global: {
        plugins: [createPinia()],
        stubs: shadcnStubs,
      },
    })
    await flushPromises()

    expect(fetchSeasonPreview).toHaveBeenCalledWith(
      3, // season_ratings_limit
      expect.any(String), // ratings_order
      'd', // season_badge_style
      'o', // season_label_style
      'm', // season_badge_size
      'bc', // season_position
      'd', // season_badge_direction
      '', // ratings_exclude
      'r', // season_badge_shape
      'd', // season_badge_background
    )
  })

  it('renders season position and badge controls but no blur control', () => {
    const fetchSeasonPreview = makeFetchPreview()
    const settings = { ...defaultSettings }
    const wrapper = mount(RenderSettingsForm, {
      props: {
        settings,
        loadSettings: vi.fn().mockResolvedValue(settings),
        saveSettings: vi.fn().mockResolvedValue(null),
        fetchPreview: makeFetchPreview(),
        fetchSeasonPreview,
      },
      global: {
        plugins: [createPinia()],
        stubs: shadcnStubs,
      },
    })
    expect(wrapper.find('[data-testid="season-position-select"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="season-badge-style-select"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="season-badge-direction-select"]').exists()).toBe(true)
    // Season has no blur control (unlike episode).
    expect(wrapper.find('[data-testid="season-blur-checkbox"]').exists()).toBe(false)
  })

  it('auto-save payload includes season fields', async () => {
    const saveSettings = vi.fn().mockResolvedValue(null)
    const settings = { ...defaultSettings }
    const wrapper = mount(RenderSettingsForm, {
      props: {
        settings,
        loadSettings: vi.fn().mockResolvedValue(settings),
        saveSettings,
        fetchPreview: makeFetchPreview(),
        fetchSeasonPreview: makeFetchPreview(),
      },
      global: {
        plugins: [createPinia()],
        stubs: shadcnStubs,
      },
    })

    // Toggle textless to trigger an auto-save.
    await wrapper.find('[data-testid="textless-checkbox"]').setValue(true)
    await flushPromises()

    expect(saveSettings).toHaveBeenCalledWith(
      expect.objectContaining({
        season_ratings_limit: 3,
        season_badge_style: 'd',
        season_label_style: 'o',
        season_badge_size: 'm',
        season_position: 'bc',
        season_badge_direction: 'd',
        season_badge_shape: 'r',
        season_badge_background: 'd',
      }),
    )
    // Season carries no blur field.
    expect(saveSettings).toHaveBeenCalledWith(
      expect.not.objectContaining({ season_blur: expect.anything() }),
    )
  })

  // --- Backdrop edge inset ---

  function mountWithBackdrop(
    overrides: Partial<RenderSettings> = {},
    fetchBackdropPreview = makeFetchPreview(),
    saveSettings = vi.fn().mockResolvedValue(null),
  ) {
    const settings = { ...defaultSettings, ...overrides }
    return mount(RenderSettingsForm, {
      props: {
        settings,
        loadSettings: vi.fn().mockResolvedValue(settings),
        saveSettings,
        fetchPreview: makeFetchPreview(),
        fetchBackdropPreview,
      },
      global: {
        plugins: [createPinia()],
        stubs: shadcnStubs,
      },
    })
  }

  it('shows both edge-inset inputs for a corner backdrop position', () => {
    const wrapper = mountWithBackdrop({ backdrop_position: 'tr' })
    expect(wrapper.find('[data-testid="backdrop-edge-inset-x"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="backdrop-edge-inset-y"]').exists()).toBe(true)
    expect(wrapper.text()).toContain('Space from top')
    expect(wrapper.text()).toContain('Space from right')
  })

  it('shows only the vertical edge-inset input for a top-center position', () => {
    const wrapper = mountWithBackdrop({ backdrop_position: 'tc' })
    expect(wrapper.find('[data-testid="backdrop-edge-inset-x"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="backdrop-edge-inset-y"]').exists()).toBe(true)
    expect(wrapper.text()).toContain('Space from top')
  })

  it('shows only the horizontal edge-inset input for a right position', () => {
    const wrapper = mountWithBackdrop({ backdrop_position: 'r' })
    expect(wrapper.find('[data-testid="backdrop-edge-inset-x"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="backdrop-edge-inset-y"]').exists()).toBe(false)
    expect(wrapper.text()).toContain('Space from right')
  })

  it('passes edge insets to fetchBackdropPreview', async () => {
    const fetchBackdropPreview = makeFetchPreview()
    mountWithBackdrop({ backdrop_position: 'tr', backdrop_edge_inset_x: 12, backdrop_edge_inset_y: 7 }, fetchBackdropPreview)
    await flushPromises()

    expect(fetchBackdropPreview).toHaveBeenCalledWith(
      3, // backdrop_ratings_limit
      expect.any(String), // ratings_order
      'v', // backdrop_badge_style
      'i', // backdrop_label_style
      'm', // backdrop_badge_size
      'tr', // backdrop_position
      'v', // backdrop_badge_direction
      '', // ratings_exclude
      'r', // backdrop_badge_shape
      'd', // backdrop_badge_background
      12, // backdrop_edge_inset_x
      7, // backdrop_edge_inset_y
    )
  })

  it('editing a backdrop edge inset auto-saves the new value', async () => {
    const saveSettings = vi.fn().mockResolvedValue(null)
    const wrapper = mountWithBackdrop({ backdrop_position: 'tr' }, makeFetchPreview(), saveSettings)

    await wrapper.find('[data-testid="backdrop-edge-inset-y"]').setValue(15)
    await flushPromises()

    expect(saveSettings).toHaveBeenCalledWith(
      expect.objectContaining({ backdrop_edge_inset_y: 15 }),
    )
  })
})
