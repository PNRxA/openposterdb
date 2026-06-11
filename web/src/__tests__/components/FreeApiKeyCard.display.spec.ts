import { describe, it, expect, afterEach, vi } from 'vitest'
import { mount, flushPromises, VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import FreeApiKeyCard from '@/components/FreeApiKeyCard.vue'
import { Select } from '@/components/ui/select'
import { useAuthStore } from '@/stores/auth'
import { DEFAULT_RATINGS_ORDER } from '@/lib/constants'
import type { FreeKeyDefaults } from '@/lib/auth-api'

/**
 * These tests mount the card with the REAL reka-ui Select components (the rest
 * of the suite stubs them). They guard the *displayed* trigger text, which has
 * its own failure mode: reka-ui snapshots each option's text once when the item
 * mounts, so a dynamic "default (resolved)" label that changes per image type
 * would otherwise go stale on the closed trigger when switching poster<->backdrop
 * (the control stays mounted, unlike logo which unmounts it).
 */
function makeDefaults(overrides: Partial<FreeKeyDefaults> = {}): FreeKeyDefaults {
  return {
    image_source: 't', lang: 'en', textless: false, ratings_limit: 3,
    ratings_order: DEFAULT_RATINGS_ORDER, ratings_exclude: '',
    poster_position: 'bc', logo_ratings_limit: 5, backdrop_ratings_limit: 5,
    poster_badge_style: 'v', logo_badge_style: 'v', backdrop_badge_style: 'h',
    poster_label_style: 'o', logo_label_style: 'o', backdrop_label_style: 'o',
    poster_badge_direction: 'd', poster_badge_split: false,
    poster_badge_size: 'm', logo_badge_size: 'm', backdrop_badge_size: 'm',
    backdrop_position: 'tc', backdrop_badge_direction: 'd',
    backdrop_edge_inset_x: 0, backdrop_edge_inset_y: 0,
    episode_ratings_limit: 3, episode_badge_style: 'v', episode_label_style: 'o',
    episode_badge_size: 'm', episode_position: 'bc', episode_badge_direction: 'd',
    episode_blur: false, poster_fit: 'native',
    season_ratings_limit: 3, season_badge_style: 'v', season_label_style: 'o',
    season_badge_size: 'm', season_position: 'bc', season_badge_direction: 'd',
    ...overrides,
  }
}

function mountReal(defaults: FreeKeyDefaults | null = makeDefaults()) {
  const pinia = createPinia()
  setActivePinia(pinia)
  const auth = useAuthStore()
  auth.freeApiKeyEnabled = true
  if (defaults) auth.freeKeyDefaults = defaults
  return mount(FreeApiKeyCard, {
    global: {
      plugins: [pinia],
      // Keep Select real; only stub the collapsible wrapper + icons.
      stubs: {
        Collapsible: { template: '<div><slot /></div>', props: ['open'] },
        CollapsibleTrigger: { template: '<div><slot /></div>', props: ['asChild'] },
        CollapsibleContent: { template: '<div><slot /></div>' },
        ChevronRight: { template: '<svg />' },
        Loader2: { template: '<svg />' },
      },
    },
  })
}

async function setSelect(wrapper: VueWrapper, triggerId: string, value: string) {
  const parent = wrapper.findAllComponents(Select).find(s => s.find(`#${triggerId}`).exists())
  if (!parent) throw new Error(`Select for #${triggerId} not found`)
  parent.vm.$emit('update:modelValue', value)
  await flushPromises()
}

const triggerText = (wrapper: VueWrapper, id: string) => wrapper.find(`#${id}`).text()

describe('FreeApiKeyCard trigger display across image-type switches', () => {
  afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals() })

  it('clears a chosen position override from the trigger when switching type', async () => {
    const wrapper = mountReal()
    await flushPromises()
    expect(triggerText(wrapper, 'free-image-position')).toBe('Position: default (Bottom Center)')

    await setSelect(wrapper, 'free-image-position', 'tr')
    expect(triggerText(wrapper, 'free-image-position')).toBe('Top Right')

    // Switching to backdrop must drop the poster override (no longer "Top Right").
    await setSelect(wrapper, 'free-image-type', 'backdrop')
    expect(triggerText(wrapper, 'free-image-position')).not.toContain('Top Right')
  })

  it('updates the resolved default annotation per type (poster<->backdrop, no unmount)', async () => {
    // backdrop_position=tc, poster_position=bc; the position control stays mounted
    // across this switch, so this guards the reka-ui stale-text path specifically.
    const wrapper = mountReal()
    await flushPromises()
    expect(triggerText(wrapper, 'free-image-position')).toBe('Position: default (Bottom Center)')

    await setSelect(wrapper, 'free-image-type', 'backdrop')
    expect(triggerText(wrapper, 'free-image-position')).toBe('Position: default (Top Center)')

    await setSelect(wrapper, 'free-image-type', 'poster')
    expect(triggerText(wrapper, 'free-image-position')).toBe('Position: default (Bottom Center)')
  })

  it('updates the badge-style default annotation per type', async () => {
    // poster_badge_style=v (Vertical), backdrop_badge_style=h (Horizontal)
    const wrapper = mountReal()
    await flushPromises()
    expect(triggerText(wrapper, 'free-badge-style')).toBe('Badge style: default (Vertical)')

    await setSelect(wrapper, 'free-image-type', 'backdrop')
    expect(triggerText(wrapper, 'free-badge-style')).toBe('Badge style: default (Horizontal)')
  })

  it('updates the badge-style default annotation when switching to season', async () => {
    // poster_badge_style=v (Vertical), season_badge_style=h (Horizontal)
    const wrapper = mountReal(makeDefaults({ poster_badge_style: 'v', season_badge_style: 'h' }))
    await flushPromises()
    expect(triggerText(wrapper, 'free-badge-style')).toBe('Badge style: default (Vertical)')

    await setSelect(wrapper, 'free-image-type', 'season')
    expect(triggerText(wrapper, 'free-badge-style')).toBe('Badge style: default (Horizontal)')
  })

  it('reflects the poster fit default annotation', async () => {
    const wrapper = mountReal(makeDefaults({ poster_fit: 'cover' }))
    await flushPromises()
    expect(triggerText(wrapper, 'free-fit')).toBe('Fit: default (Crop to 2:3)')
  })

  it('reflects the server default annotation once defaults load after mount', async () => {
    // Mount with no defaults (null): the trigger shows the bare label. When the
    // store resolves defaults, the keyed item re-registers and the annotation
    // appears on the closed trigger without any user interaction.
    const wrapper = mountReal(null)
    await flushPromises()
    expect(triggerText(wrapper, 'free-badge-style')).toBe('Badge style: default')

    useAuthStore().freeKeyDefaults = makeDefaults({ poster_badge_style: 'h' })
    await flushPromises()
    expect(triggerText(wrapper, 'free-badge-style')).toBe('Badge style: default (Horizontal)')
  })
})
