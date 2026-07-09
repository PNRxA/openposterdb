import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises, VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import FreeApiKeyCard from '@/components/FreeApiKeyCard.vue'
import RatingsOrderList from '@/components/RatingsOrderList.vue'
import { useAuthStore } from '@/stores/auth'
import { DEFAULT_RATINGS_ORDER } from '@/lib/constants'
import type { FreeKeyDefaults } from '@/lib/auth-api'

vi.mock('@/stores/auth', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/stores/auth')>()
  return actual
})

/** A full FreeKeyDefaults fixture; override individual fields per test. */
function makeDefaults(overrides: Partial<FreeKeyDefaults> = {}): FreeKeyDefaults {
  return {
    image_source: 't',
    lang: 'en',
    textless: false,
    ratings_limit: 3,
    ratings_order: DEFAULT_RATINGS_ORDER,
    ratings_exclude: '',
    poster_position: 'bc',
    logo_ratings_limit: 5,
    backdrop_ratings_limit: 5,
    poster_badge_style: 'v',
    logo_badge_style: 'v',
    backdrop_badge_style: 'v',
    poster_label_style: 'o',
    logo_label_style: 'o',
    backdrop_label_style: 'o',
    poster_badge_direction: 'd',
    poster_badge_split: false,
    poster_fit: 'native',
    poster_badge_size: 'm',
    logo_badge_size: 'm',
    backdrop_badge_size: 'm',
    backdrop_position: 'bc',
    backdrop_badge_direction: 'd',
    backdrop_edge_inset_x: 0,
    backdrop_edge_inset_y: 0,
    episode_ratings_limit: 3,
    episode_badge_style: 'v',
    episode_label_style: 'o',
    episode_badge_size: 'm',
    episode_position: 'bc',
    episode_badge_direction: 'd',
    episode_blur: false,
    quality_style: 'text',
    quality_direction: 'd',
    poster_lang_icon: 'off',
    logo_lang_icon: 'off',
    backdrop_lang_icon: 'off',
    lang_exclude: '',
    poster_quality_position: 'tr',
    backdrop_quality_position: 'tl',
    poster_lang_position: 'tl',
    backdrop_lang_position: 'bl',
    ...overrides,
  }
}

const SelectStub = {
  name: 'Select',
  template: '<div data-stub="select"><slot /></div>',
  props: ['modelValue'],
  emits: ['update:modelValue'],
}

function mountCard(freeApiKeyEnabled = true, defaults: FreeKeyDefaults | null = makeDefaults()) {
  const pinia = createPinia()
  setActivePinia(pinia)
  const auth = useAuthStore()
  auth.freeApiKeyEnabled = freeApiKeyEnabled
  // Pre-seed so the card's onMounted load short-circuits (no network in tests).
  // Pass `null` to exercise the fetch/fallback paths explicitly.
  if (defaults) auth.freeKeyDefaults = defaults

  return mount(FreeApiKeyCard, {
    global: {
      plugins: [pinia],
      stubs: {
        Select: SelectStub,
        SelectTrigger: { template: '<span :id="id"><slot /></span>', props: ['id', 'ariaLabel', 'class'] },
        SelectValue: { template: '<span>{{ placeholder }}</span>', props: ['placeholder'] },
        SelectContent: { template: '<span><slot /></span>' },
        SelectItem: { template: '<span><slot /></span>', props: ['value'] },
        Collapsible: { template: '<div><slot /></div>', props: ['open'] },
        CollapsibleTrigger: { template: '<div><slot /></div>', props: ['asChild'] },
        CollapsibleContent: { template: '<div><slot /></div>' },
        Input: {
          name: 'Input',
          // Mirror Vue's real v-model coercion: a `type="number"` input yields a
          // Number (the real shadcn Input wraps a native input, so the card's
          // refs receive numbers — string-only handling would break at runtime).
          template: '<input :value="modelValue" :placeholder="placeholder" @input="onInput" />',
          props: ['modelValue', 'type', 'placeholder', 'required', 'id'],
          emits: ['update:modelValue'],
          methods: {
            onInput(e: Event) {
              const raw = (e.target as HTMLInputElement).value
              const val = this.type === 'number' && raw !== '' && !Number.isNaN(Number(raw)) ? Number(raw) : raw
              this.$emit('update:modelValue', val)
            },
          },
        },
        Button: {
          template: '<button :disabled="disabled" :type="type" @click="$emit(\'click\')"><slot /></button>',
          props: ['disabled', 'variant', 'size', 'type'],
        },
        Checkbox: {
          name: 'Checkbox',
          template: '<button type="button" role="checkbox" :id="id" :aria-checked="modelValue ? \'true\' : \'false\'" @click="$emit(\'update:modelValue\', !modelValue)" />',
          props: ['modelValue', 'id', 'ariaLabel'],
          emits: ['update:modelValue'],
        },
        Label: { template: '<label><slot /></label>', props: ['for'] },
        ChevronRight: { template: '<svg />' },
        Loader2: { template: '<svg />' },
      },
    },
  })
}

/**
 * Set a Select's value by finding its SelectTrigger via id,
 * then emitting on the parent Select component.
 */
async function setSelectById(wrapper: VueWrapper, triggerId: string, value: string) {
  const trigger = wrapper.find(`#${triggerId}`)
  if (!trigger.exists()) {
    throw new Error(`SelectTrigger with id="${triggerId}" not found`)
  }
  // Walk up to find the parent Select stub component
  const selectComponents = wrapper.findAllComponents(SelectStub)
  const parent = selectComponents.find(s => s.find(`#${triggerId}`).exists())
  if (!parent) {
    throw new Error(`Parent Select for trigger id="${triggerId}" not found`)
  }
  parent.vm.$emit('update:modelValue', value)
  await flushPromises()
}

/** Get the curl example code element (the last code element). */
function findCurlCode(wrapper: VueWrapper) {
  const codes = wrapper.findAll('code')
  return codes[codes.length - 1]
}

describe('FreeApiKeyCard', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('renders nothing when freeApiKeyEnabled is false', () => {
    const wrapper = mountCard(false)
    expect(wrapper.text()).toBe('')
  })

  it('renders card with FREE_API_KEY code when enabled', () => {
    const wrapper = mountCard(true)
    expect(wrapper.text()).toContain('t0-free-rpdb')
    expect(wrapper.text()).toContain('Free API Key Available')
  })

  it('offers ratings-limit options up to 10 badges (one per rating source)', () => {
    const wrapper = mountCard(true)
    const ratingsSelect = wrapper
      .findAllComponents(SelectStub)
      .find((s) => s.find('#free-ratings-limit').exists())
    expect(ratingsSelect).toBeTruthy()
    const text = ratingsSelect!.text()
    expect(text).toContain('1 badge')
    expect(text).toContain('9 badges')
    expect(text).toContain('10 badges')
    // 10 is the backend max (validate_ratings_limit allows 0–10); don't offer more.
    expect(text).not.toContain('11 badges')
  })

  it('curlExample uses .jpg for poster and .png for logo', async () => {
    const wrapper = mountCard(true)
    expect(findCurlCode(wrapper).text()).toContain('.jpg')

    await setSelectById(wrapper, 'free-image-type', 'logo')
    expect(findCurlCode(wrapper).text()).toContain('.png')
  })

  it('sizeOptions excludes small for poster, includes small for backdrop', async () => {
    const wrapper = mountCard(true)
    // Default is poster — set "small" then switch imageType to trigger the watch
    await setSelectById(wrapper, 'free-image-size', 'small')
    // Switch imageType away to trigger the reset (small invalid for logo)
    await setSelectById(wrapper, 'free-image-type', 'logo')
    expect(findCurlCode(wrapper).text()).not.toContain('imageSize=small')

    // Switch to backdrop — "small" should be valid and persist
    await setSelectById(wrapper, 'free-image-type', 'backdrop')
    await setSelectById(wrapper, 'free-image-size', 'small')
    expect(findCurlCode(wrapper).text()).toContain('imageSize=small')
  })

  it('resets imageSize to default when switching imageType invalidates current size', async () => {
    const wrapper = mountCard(true)

    // Switch to backdrop
    await setSelectById(wrapper, 'free-image-type', 'backdrop')
    // Set size to small (valid for backdrop)
    await setSelectById(wrapper, 'free-image-size', 'small')
    expect(findCurlCode(wrapper).text()).toContain('imageSize=small')

    // Switch back to poster — "small" is invalid, should reset to default
    await setSelectById(wrapper, 'free-image-type', 'poster')
    expect(findCurlCode(wrapper).text()).not.toContain('imageSize=small')
  })

  it('handleFetch creates blob URL on success', async () => {
    const blobUrl = 'blob:http://localhost/fake'
    vi.spyOn(URL, 'createObjectURL').mockReturnValue(blobUrl)
    vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: true,
        blob: () => Promise.resolve(new Blob(['img'], { type: 'image/jpeg' })),
      }),
    )

    const wrapper = mountCard(true)
    const form = wrapper.find('form')
    await form.trigger('submit')
    await flushPromises()

    expect(URL.createObjectURL).toHaveBeenCalled()
    const img = wrapper.find('img[alt="Fetched result"]')
    expect(img.exists()).toBe(true)
    expect(img.attributes('src')).toBe(blobUrl)
  })

  it('handleFetch shows "Not found" error on 404', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({ ok: false, status: 404 }),
    )

    const wrapper = mountCard(true)
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(wrapper.text()).toContain('Not found')
  })

  it('handleFetch shows generic error on network failure', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockRejectedValue(new TypeError('Failed to fetch')),
    )

    const wrapper = mountCard(true)
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(wrapper.text()).toContain('Failed to fetch')
  })

  it('queryString includes lang, imageSize params when set', async () => {
    const wrapper = mountCard(true)

    await setSelectById(wrapper, 'free-lang', 'en')
    await setSelectById(wrapper, 'free-image-size', 'large')

    const curlText = findCurlCode(wrapper).text()
    expect(curlText).toContain('lang=en')
    expect(curlText).toContain('imageSize=large')
  })

  it('idPlaceholder changes per idType', async () => {
    const wrapper = mountCard(true)

    // The ID input is the only non-checkbox input without an aria-label (the
    // lang-code override input carries one), so exclude aria-labelled inputs.
    const getPlaceholder = () => wrapper.find('input:not([type="checkbox"]):not([aria-label])').attributes('placeholder')
    expect(getPlaceholder()).toBe('tt0013442')

    await setSelectById(wrapper, 'free-id-type', 'tmdb')
    expect(getPlaceholder()).toBe('movie-872585 or episode-1396-S1E1')

    await setSelectById(wrapper, 'free-id-type', 'tvdb')
    expect(getPlaceholder()).toBe('253573')
  })

  it('resets poster-only controls when switching away from poster', async () => {
    const wrapper = mountCard(true)

    // Set poster-only controls and general image controls
    await setSelectById(wrapper, 'free-image-position', 'tl')
    await setSelectById(wrapper, 'free-badge-direction', 'v')
    await setSelectById(wrapper, 'free-image-source', 'f')
    await setSelectById(wrapper, 'free-textless', 'true')
    expect(findCurlCode(wrapper).text()).toContain('position=tl')
    expect(findCurlCode(wrapper).text()).toContain('badge_direction=v')
    expect(findCurlCode(wrapper).text()).toContain('image_source=f')
    expect(findCurlCode(wrapper).text()).toContain('textless=true')

    // Switch to logo — poster-only controls should reset, source persists
    await setSelectById(wrapper, 'free-image-type', 'logo')
    expect(findCurlCode(wrapper).text()).not.toContain('position=')
    expect(findCurlCode(wrapper).text()).not.toContain('badge_direction=')
    expect(findCurlCode(wrapper).text()).not.toContain('textless=')
    expect(findCurlCode(wrapper).text()).toContain('image_source=f')

    // Switch back to poster — poster-only controls at defaults, source still set
    await setSelectById(wrapper, 'free-image-type', 'poster')
    expect(findCurlCode(wrapper).text()).not.toContain('position=')
    expect(findCurlCode(wrapper).text()).not.toContain('badge_direction=')
    expect(findCurlCode(wrapper).text()).not.toContain('textless=')
    expect(findCurlCode(wrapper).text()).toContain('image_source=f')
  })

  it('resets per-type render overrides when switching image type', async () => {
    const wrapper = mountCard(true)

    // Override poster's per-type render settings away from their defaults.
    await setSelectById(wrapper, 'free-badge-style', 'h')
    await setSelectById(wrapper, 'free-label-style', 't')
    await setSelectById(wrapper, 'free-badge-size', 'l')
    await setSelectById(wrapper, 'free-ratings-limit', '7')
    // A global control (lang) that should survive the switch.
    await setSelectById(wrapper, 'free-lang', 'en')
    const posterCurl = findCurlCode(wrapper).text()
    expect(posterCurl).toContain('badge_style=h')
    expect(posterCurl).toContain('label_style=t')
    expect(posterCurl).toContain('badge_size=l')
    expect(posterCurl).toContain('ratings_limit=7')

    // Switching type re-applies the new type's defaults: per-type overrides drop,
    // the global lang persists.
    await setSelectById(wrapper, 'free-image-type', 'backdrop')
    const backdropCurl = findCurlCode(wrapper).text()
    expect(backdropCurl).not.toContain('badge_style=')
    expect(backdropCurl).not.toContain('label_style=')
    expect(backdropCurl).not.toContain('badge_size=')
    expect(backdropCurl).not.toContain('ratings_limit=')
    expect(backdropCurl).toContain('lang=en')
  })

  // --- Episode support ---

  it('episode queryString includes badge_direction, position, and blur', async () => {
    const wrapper = mountCard(true)
    await setSelectById(wrapper, 'free-image-type', 'episode')
    await setSelectById(wrapper, 'free-badge-direction', 'v')
    await setSelectById(wrapper, 'free-image-position', 'tr')
    await setSelectById(wrapper, 'free-blur', 'true')

    const curlText = findCurlCode(wrapper).text()
    expect(curlText).toContain('badge_direction=v')
    expect(curlText).toContain('position=tr')
    expect(curlText).toContain('blur=true')
  })

  // --- Backdrop edge inset support ---

  it('edge inset inputs appear only for backdrop image type', async () => {
    const wrapper = mountCard(true)
    const insetX = () => wrapper.find('input[aria-label*="horizontal edge inset"]')
    // Default poster — no edge inset inputs
    expect(insetX().exists()).toBe(false)

    await setSelectById(wrapper, 'free-image-type', 'backdrop')
    expect(insetX().exists()).toBe(true)
    expect(wrapper.find('input[aria-label*="vertical edge inset"]').exists()).toBe(true)

    // Switch to logo — inputs disappear
    await setSelectById(wrapper, 'free-image-type', 'logo')
    expect(insetX().exists()).toBe(false)
  })

  it('backdrop queryString includes edge_inset_x/y when set', async () => {
    const wrapper = mountCard(true)
    await setSelectById(wrapper, 'free-image-type', 'backdrop')
    await wrapper.find('input[aria-label*="horizontal edge inset"]').setValue('12')
    await wrapper.find('input[aria-label*="vertical edge inset"]').setValue('7')

    const curl = findCurlCode(wrapper).text()
    expect(curl).toContain('edge_inset_x=12')
    expect(curl).toContain('edge_inset_y=7')
  })

  it('clamps backdrop edge inset to the accepted 0–50 range', async () => {
    const wrapper = mountCard(true)
    await setSelectById(wrapper, 'free-image-type', 'backdrop')
    await wrapper.find('input[aria-label*="horizontal edge inset"]').setValue('999')
    expect(findCurlCode(wrapper).text()).toContain('edge_inset_x=50')
  })

  it('resets backdrop edge insets when switching image type', async () => {
    const wrapper = mountCard(true)
    await setSelectById(wrapper, 'free-image-type', 'backdrop')
    await wrapper.find('input[aria-label*="horizontal edge inset"]').setValue('20')
    expect(findCurlCode(wrapper).text()).toContain('edge_inset_x=20')

    // Switch away and back — the override should have been cleared.
    await setSelectById(wrapper, 'free-image-type', 'episode')
    await setSelectById(wrapper, 'free-image-type', 'backdrop')
    expect(findCurlCode(wrapper).text()).not.toContain('edge_inset_x=')
  })

  it('blur selector only appears for episode image type', async () => {
    const wrapper = mountCard(true)

    // Default is poster — no blur select
    expect(wrapper.find('#free-blur').exists()).toBe(false)

    // Switch to episode — blur should appear
    await setSelectById(wrapper, 'free-image-type', 'episode')
    expect(wrapper.find('#free-blur').exists()).toBe(true)

    // Switch to backdrop — blur should disappear
    await setSelectById(wrapper, 'free-image-type', 'backdrop')
    expect(wrapper.find('#free-blur').exists()).toBe(false)
  })

  it('switching from episode to logo resets blur', async () => {
    const wrapper = mountCard(true)

    await setSelectById(wrapper, 'free-image-type', 'episode')
    await setSelectById(wrapper, 'free-blur', 'true')
    expect(findCurlCode(wrapper).text()).toContain('blur=true')

    // Switch to logo — blur should reset
    await setSelectById(wrapper, 'free-image-type', 'logo')
    expect(findCurlCode(wrapper).text()).not.toContain('blur=')
  })

  it('episode uses .jpg extension in curl example', async () => {
    const wrapper = mountCard(true)
    await setSelectById(wrapper, 'free-image-type', 'episode')

    const curlText = findCurlCode(wrapper).text()
    expect(curlText).toContain('episode-default/')
    expect(curlText).toContain('.jpg')
  })

  it('sizeOptions includes small for episode', async () => {
    const wrapper = mountCard(true)
    await setSelectById(wrapper, 'free-image-type', 'episode')
    await setSelectById(wrapper, 'free-image-size', 'small')
    expect(findCurlCode(wrapper).text()).toContain('imageSize=small')
  })

  it('episode keeps position and badge_direction controls', async () => {
    const wrapper = mountCard(true)
    await setSelectById(wrapper, 'free-image-type', 'episode')

    expect(wrapper.find('#free-image-position').exists()).toBe(true)
    expect(wrapper.find('#free-badge-direction').exists()).toBe(true)
  })

  it('backdrop keeps position and badge_direction controls', async () => {
    const wrapper = mountCard(true)
    await setSelectById(wrapper, 'free-image-type', 'backdrop')

    expect(wrapper.find('#free-image-position').exists()).toBe(true)
    expect(wrapper.find('#free-badge-direction').exists()).toBe(true)
  })

  it('backdrop queryString includes position and badge_direction', async () => {
    const wrapper = mountCard(true)
    await setSelectById(wrapper, 'free-image-type', 'backdrop')
    await setSelectById(wrapper, 'free-image-position', 'tl')
    await setSelectById(wrapper, 'free-badge-direction', 'h')

    const curlText = findCurlCode(wrapper).text()
    expect(curlText).toContain('position=tl')
    expect(curlText).toContain('badge_direction=h')
  })

  it('switching from backdrop to logo resets position and badge_direction', async () => {
    const wrapper = mountCard(true)
    await setSelectById(wrapper, 'free-image-type', 'backdrop')
    await setSelectById(wrapper, 'free-image-position', 'tl')
    await setSelectById(wrapper, 'free-badge-direction', 'h')
    expect(findCurlCode(wrapper).text()).toContain('position=tl')

    await setSelectById(wrapper, 'free-image-type', 'logo')
    expect(findCurlCode(wrapper).text()).not.toContain('position=')
    expect(findCurlCode(wrapper).text()).not.toContain('badge_direction=')
  })

  it('episode does not show textless control', async () => {
    const wrapper = mountCard(true)
    await setSelectById(wrapper, 'free-image-type', 'episode')

    expect(wrapper.find('#free-textless').exists()).toBe(false)
  })

  it('adds the poster fit override to the query and clears it for non-poster types', async () => {
    const wrapper = mountCard(true)
    await setSelectById(wrapper, 'free-fit', 'cover')
    expect(findCurlCode(wrapper).text()).toContain('fit=cover')

    await setSelectById(wrapper, 'free-image-type', 'logo')
    expect(findCurlCode(wrapper).text()).not.toContain('fit=')
  })

  it('episode does not show fit control', async () => {
    const wrapper = mountCard(true)
    await setSelectById(wrapper, 'free-image-type', 'episode')

    expect(wrapper.find('#free-fit').exists()).toBe(false)
  })

  // --- Reflecting the server's global defaults ---

  /** Rating-priority row labels, in display order. */
  function orderLabels(wrapper: VueWrapper) {
    return wrapper.findComponent(RatingsOrderList).findAll('span.flex-1')
  }

  it('seeds the rating priority list from the server order', () => {
    const wrapper = mountCard(true, makeDefaults({ ratings_order: 'tmdb,imdb,rt' }))
    const labels = orderLabels(wrapper)
    expect(labels[0].text()).toBe('TMDB')
    expect(labels[1].text()).toBe('IMDb')
  })

  it('annotates dropdown defaults with the server values', () => {
    const wrapper = mountCard(true, makeDefaults({
      poster_badge_style: 'v',
      poster_label_style: 't',
      poster_badge_size: 'l',
      ratings_limit: 5,
      image_source: 'f',
      lang: 'de',
    }))
    const text = wrapper.text()
    expect(text).toContain('Badge style: default (Vertical)')
    expect(text).toContain('Label style: default (Text)')
    expect(text).toContain('Badge size: default (Large)')
    expect(text).toContain('Max badges: default (5)')
    expect(text).toContain('Source: default (Fanart.tv)')
    expect(text).toContain('Language: any (de)')
  })

  it('reflects per-image-type defaults when switching image type', async () => {
    const wrapper = mountCard(true, makeDefaults({ poster_badge_style: 'v', logo_badge_style: 'h' }))
    expect(wrapper.text()).toContain('Badge style: default (Vertical)')

    await setSelectById(wrapper, 'free-image-type', 'logo')
    expect(wrapper.text()).toContain('Badge style: default (Horizontal)')
  })

  it('dims excluded sources in the priority list and pre-checks them', () => {
    const wrapper = mountCard(true, makeDefaults({ ratings_exclude: 'rt' }))
    const rtLabel = orderLabels(wrapper).find(s => s.text() === 'Rotten Tomatoes (Critics)')
    expect(rtLabel?.classes()).toContain('line-through')
    // The exclude checkbox for rt is pre-checked from the server baseline.
    expect(wrapper.find('#free-exclude-rt').attributes('aria-checked')).toBe('true')
    expect(wrapper.find('#free-exclude-imdb').attributes('aria-checked')).toBe('false')
  })

  it('does not send ratings_exclude when the selection matches the server baseline', () => {
    const wrapper = mountCard(true, makeDefaults({ ratings_exclude: 'rt' }))
    expect(findCurlCode(wrapper).text()).not.toContain('ratings_exclude=')
  })

  it('excluding a source adds it to the curl and dims it in the list', async () => {
    const wrapper = mountCard(true, makeDefaults({ ratings_exclude: '' }))
    await wrapper.find('#free-exclude-rt').trigger('click')
    await flushPromises()

    expect(findCurlCode(wrapper).text()).toContain('ratings_exclude=rt')
    const rtLabel = orderLabels(wrapper).find(s => s.text() === 'Rotten Tomatoes (Critics)')
    expect(rtLabel?.classes()).toContain('line-through')
  })

  it('unchecking a server exclusion emits an empty ratings_exclude override', async () => {
    const wrapper = mountCard(true, makeDefaults({ ratings_exclude: 'rt' }))
    await wrapper.find('#free-exclude-rt').trigger('click')
    await flushPromises()

    // Empty value (trailing `=`) is required to override the server's exclusion.
    expect(findCurlCode(wrapper).text()).toContain('ratings_exclude=')
    expect(findCurlCode(wrapper).text()).not.toContain('ratings_exclude=rt')
  })

  it('does not send ratings_order when the list matches the server order', () => {
    const wrapper = mountCard(true, makeDefaults({ ratings_order: 'tmdb,imdb,rt' }))
    expect(findCurlCode(wrapper).text()).not.toContain('ratings_order=')
  })

  it('sends ratings_order once the user reorders away from the server order', async () => {
    const wrapper = mountCard(true, makeDefaults({ ratings_order: 'tmdb,imdb,rt' }))
    // Move the second item (imdb) above tmdb — diverges from the server baseline.
    const upButtons = wrapper.findAll('button[title="Move up"]')
    await upButtons[1].trigger('click')
    await flushPromises()
    expect(findCurlCode(wrapper).text()).toContain('ratings_order=imdb%2Ctmdb')
  })

  it('falls back to built-in defaults when server settings are unavailable', () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('offline')))
    const wrapper = mountCard(true, null)
    const text = wrapper.text()
    expect(text).toContain('Badge style: default')
    expect(text).not.toContain('Badge style: default (')
    expect(findCurlCode(wrapper).text()).not.toContain('ratings_order=')
  })

  // --- Quality + main-language overlay badges ---

  it('adds quality only when at least one tier is selected, stacking in canonical order', async () => {
    const wrapper = mountCard(true)
    // No tiers selected → no quality param.
    expect(findCurlCode(wrapper).text()).not.toContain('quality=')

    // Select dv first, then 4k — the emitted value follows canonical tier order.
    await wrapper.find('#free-quality-dv').trigger('click')
    await wrapper.find('#free-quality-4k').trigger('click')
    await flushPromises()
    expect(findCurlCode(wrapper).text()).toContain('quality=4k%2Cdv')
  })

  it('adds quality_style only when overridden away from default', async () => {
    const wrapper = mountCard(true)
    expect(findCurlCode(wrapper).text()).not.toContain('quality_style=')

    await setSelectById(wrapper, 'free-quality-style', 'logo')
    expect(findCurlCode(wrapper).text()).toContain('quality_style=logo')
  })

  it('adds lang_icon whenever overridden away from default (including off)', async () => {
    const wrapper = mountCard(true)
    expect(findCurlCode(wrapper).text()).not.toContain('lang_icon=')

    // Explicitly choosing Off must emit the param, so it overrides a non-off
    // server default (e.g. an admin who set the global default to flag/text).
    await setSelectById(wrapper, 'free-lang-icon', 'off')
    expect(findCurlCode(wrapper).text()).toContain('lang_icon=off')

    await setSelectById(wrapper, 'free-lang-icon', 'flag')
    expect(findCurlCode(wrapper).text()).toContain('lang_icon=flag')
  })

  it('adds lang_code only when non-empty and a language icon is enabled', async () => {
    const wrapper = mountCard(true)
    // lang_code with no icon active → omitted.
    await wrapper.find('input[aria-label*="ISO 639-1"]').setValue('ja')
    expect(findCurlCode(wrapper).text()).not.toContain('lang_code=')

    // Enable an icon → lang_code is now sent.
    await setSelectById(wrapper, 'free-lang-icon', 'text')
    expect(findCurlCode(wrapper).text()).toContain('lang_code=ja')
  })

  it('renders the lang-exclude input', () => {
    const wrapper = mountCard(true)
    expect(wrapper.find('input[aria-label*="exclude ISO 639-1"]').exists()).toBe(true)
  })

  it('adds lang_exclude only when non-empty and a language icon is enabled', async () => {
    const wrapper = mountCard(true)
    // lang_exclude with no icon active → omitted.
    await wrapper.find('input[aria-label*="exclude ISO 639-1"]').setValue('en,es')
    expect(findCurlCode(wrapper).text()).not.toContain('lang_exclude=')

    // Enable an icon → lang_exclude is now sent.
    await setSelectById(wrapper, 'free-lang-icon', 'flag')
    expect(findCurlCode(wrapper).text()).toContain('lang_exclude=en%2Ces')
  })

  it('quality tiers persist across image-type switches (global control)', async () => {
    const wrapper = mountCard(true)
    await wrapper.find('#free-quality-4k').trigger('click')
    await flushPromises()
    expect(findCurlCode(wrapper).text()).toContain('quality=4k')

    await setSelectById(wrapper, 'free-image-type', 'logo')
    expect(findCurlCode(wrapper).text()).toContain('quality=4k')
  })

  it('remembers the language icon per image type', async () => {
    const wrapper = mountCard(true)

    // Poster value.
    await setSelectById(wrapper, 'free-lang-icon', 'flag')
    expect(findCurlCode(wrapper).text()).toContain('lang_icon=flag')

    // Switch to logo — the poster override no longer applies; set a distinct logo
    // value and confirm it's the one emitted for logo.
    await setSelectById(wrapper, 'free-image-type', 'logo')
    expect(findCurlCode(wrapper).text()).not.toContain('lang_icon=flag')
    await setSelectById(wrapper, 'free-lang-icon', 'text')
    expect(findCurlCode(wrapper).text()).toContain('lang_icon=text')

    // Switch back to poster — the poster value is remembered per type.
    await setSelectById(wrapper, 'free-image-type', 'poster')
    expect(findCurlCode(wrapper).text()).toContain('lang_icon=flag')
  })

  it('hides the lang-icon select for episode and never emits lang_icon there', async () => {
    const wrapper = mountCard(true)
    await setSelectById(wrapper, 'free-image-type', 'episode')
    expect(wrapper.find('#free-lang-icon').exists()).toBe(false)
    expect(findCurlCode(wrapper).text()).not.toContain('lang_icon=')
  })

  it('renders the lang-icon select for poster, logo, and backdrop', async () => {
    const wrapper = mountCard(true)
    expect(wrapper.find('#free-lang-icon').exists()).toBe(true)

    await setSelectById(wrapper, 'free-image-type', 'logo')
    expect(wrapper.find('#free-lang-icon').exists()).toBe(true)

    await setSelectById(wrapper, 'free-image-type', 'backdrop')
    expect(wrapper.find('#free-lang-icon').exists()).toBe(true)
  })

  it('annotates quality_style and the per-type lang_icon default with the server values', async () => {
    const wrapper = mountCard(true, makeDefaults({
      quality_style: 'logo',
      poster_lang_icon: 'flag',
      logo_lang_icon: 'text',
      backdrop_lang_icon: 'off',
    }))
    // Default type is poster — labels reflect the poster server defaults.
    let text = wrapper.text()
    expect(text).toContain('Quality style: default (Logo)')
    expect(text).toContain('Language icon: default (Flag)')

    // Switching to logo reflects the logo server default.
    await setSelectById(wrapper, 'free-image-type', 'logo')
    text = wrapper.text()
    expect(text).toContain('Language icon: default (Text)')

    // And backdrop reflects the backdrop server default.
    await setSelectById(wrapper, 'free-image-type', 'backdrop')
    text = wrapper.text()
    expect(text).toContain('Language icon: default (Off)')
  })

  it('renders the quality-position and lang-position selects for poster', () => {
    const wrapper = mountCard(true)
    expect(wrapper.find('#free-quality-position').exists()).toBe(true)
    expect(wrapper.find('#free-lang-position').exists()).toBe(true)
  })

  it('renders the quality-position and lang-position selects for backdrop', async () => {
    const wrapper = mountCard(true)
    await setSelectById(wrapper, 'free-image-type', 'backdrop')
    expect(wrapper.find('#free-quality-position').exists()).toBe(true)
    expect(wrapper.find('#free-lang-position').exists()).toBe(true)
  })

  it('hides the overlay-position selects for logo and episode (positions ignored)', async () => {
    const wrapper = mountCard(true)
    await setSelectById(wrapper, 'free-image-type', 'logo')
    expect(wrapper.find('#free-quality-position').exists()).toBe(false)
    expect(wrapper.find('#free-lang-position').exists()).toBe(false)

    await setSelectById(wrapper, 'free-image-type', 'episode')
    expect(wrapper.find('#free-quality-position').exists()).toBe(false)
    expect(wrapper.find('#free-lang-position').exists()).toBe(false)
  })

  it('renders the quality-direction select', () => {
    const wrapper = mountCard(true)
    expect(wrapper.find('#free-quality-direction').exists()).toBe(true)
  })

  it('adds quality_direction only when a quality tier is active and the direction is overridden', async () => {
    const wrapper = mountCard(true)
    // No tiers selected → direction is irrelevant and omitted even when set.
    await setSelectById(wrapper, 'free-quality-direction', 'v')
    expect(findCurlCode(wrapper).text()).not.toContain('quality_direction=')

    // Select a tier so the quality overlay renders; direction is now emitted.
    await wrapper.find('#free-quality-4k').trigger('click')
    await flushPromises()
    expect(findCurlCode(wrapper).text()).toContain('quality_direction=v')
  })

  it('omits quality_direction while it matches the default sentinel', async () => {
    const wrapper = mountCard(true)
    await wrapper.find('#free-quality-4k').trigger('click')
    await flushPromises()
    // Tier active but direction left at 'default' → no param.
    expect(findCurlCode(wrapper).text()).not.toContain('quality_direction=')
  })

  it('annotates the quality_direction default with the server value', () => {
    const wrapper = mountCard(true, makeDefaults({ quality_direction: 'v' }))
    expect(wrapper.text()).toContain('Quality direction: default (Vertical)')
  })

  it('adds quality_position only when a quality tier is active and the position is overridden', async () => {
    const wrapper = mountCard(true)
    // No tiers selected → position is irrelevant and omitted even when set.
    await setSelectById(wrapper, 'free-quality-position', 'bl')
    expect(findCurlCode(wrapper).text()).not.toContain('quality_position=')

    // Select a tier so the quality overlay renders; position is now emitted.
    await wrapper.find('#free-quality-4k').trigger('click')
    await flushPromises()
    expect(findCurlCode(wrapper).text()).toContain('quality_position=bl')
  })

  it('emits the per-image-type quality_position for the selected type', async () => {
    const wrapper = mountCard(true)
    await wrapper.find('#free-quality-4k').trigger('click')
    await flushPromises()

    // Poster value.
    await setSelectById(wrapper, 'free-quality-position', 'bl')
    expect(findCurlCode(wrapper).text()).toContain('quality_position=bl')

    // Switch to backdrop — the poster override no longer applies; set a distinct
    // backdrop value and confirm it's the one emitted for backdrop.
    await setSelectById(wrapper, 'free-image-type', 'backdrop')
    expect(findCurlCode(wrapper).text()).not.toContain('quality_position=bl')
    await setSelectById(wrapper, 'free-quality-position', 'tc')
    expect(findCurlCode(wrapper).text()).toContain('quality_position=tc')

    // Switch back to poster — the poster value is remembered per type.
    await setSelectById(wrapper, 'free-image-type', 'poster')
    expect(findCurlCode(wrapper).text()).toContain('quality_position=bl')
  })

  it('omits quality_position while it matches the default sentinel', async () => {
    const wrapper = mountCard(true)
    await wrapper.find('#free-quality-4k').trigger('click')
    await flushPromises()
    // Tier active but position left at 'default' → no param.
    expect(findCurlCode(wrapper).text()).not.toContain('quality_position=')
  })

  it('adds lang_position only when a language icon is active and the position is overridden', async () => {
    const wrapper = mountCard(true)
    // Icon off/default → position omitted even when set.
    await setSelectById(wrapper, 'free-lang-position', 'br')
    expect(findCurlCode(wrapper).text()).not.toContain('lang_position=')

    // Enable a language icon → position is now emitted.
    await setSelectById(wrapper, 'free-lang-icon', 'flag')
    expect(findCurlCode(wrapper).text()).toContain('lang_position=br')
  })

  it('annotates per-type quality_position and lang_position defaults with the server values', async () => {
    const wrapper = mountCard(true, makeDefaults({
      poster_quality_position: 'bl',
      poster_lang_position: 'br',
      backdrop_quality_position: 'tc',
      backdrop_lang_position: 'l',
    }))
    // Default type is poster — labels reflect the poster server defaults.
    let text = wrapper.text()
    expect(text).toContain('Quality position: default (Bottom Left)')
    expect(text).toContain('Language position: default (Bottom Right)')

    // Switching to backdrop reflects the backdrop server defaults.
    await setSelectById(wrapper, 'free-image-type', 'backdrop')
    text = wrapper.text()
    expect(text).toContain('Quality position: default (Top Center)')
    expect(text).toContain('Language position: default (Left)')
  })

  it('loads and reflects server defaults via the API on mount', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(makeDefaults({ poster_badge_style: 'h', lang: 'fr' })),
      }),
    )
    const wrapper = mountCard(true, null)
    await flushPromises()

    const text = wrapper.text()
    expect(text).toContain('Badge style: default (Horizontal)')
    expect(text).toContain('Language: any (fr)')
  })
})
