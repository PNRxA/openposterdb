<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { useAuthStore } from '@/stores/auth'
import {
  FREE_API_KEY,
  LANGUAGES,
  ALL_RATING_SOURCES,
  DEFAULT_RATINGS_ORDER,
  parseRatingsOrder,
  parseRatingsExclude,
  BADGE_STYLE_LABELS,
  BADGE_DIRECTION_LABELS,
  LABEL_STYLE_LABELS,
  BADGE_SIZE_LABELS,
  BADGE_SHAPE_LABELS,
  BADGE_BACKGROUND_LABELS,
  IMAGE_SOURCE_LABELS,
  POSITION_LABELS,
  POSTER_FIT_LABELS,
  QUALITY_TIERS,
  QUALITY_STYLE_LABELS,
  LANG_ICON_LABELS,
} from '@/lib/constants'
import RatingsOrderList from '@/components/RatingsOrderList.vue'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Label } from '@/components/ui/label'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import { ChevronRight, Loader2 } from 'lucide-vue-next'

const isOpen = ref(false)

const auth = useAuthStore()

// Server's global default render settings, so the form reflects what the free
// key actually produces rather than hardcoded frontend defaults. Loaded lazily
// (and cached) by the store; `defaults` stays null until it resolves, in which
// case every control gracefully falls back to its built-in default.
const defaults = computed(() => auth.freeKeyDefaults)

onMounted(() => {
  if (auth.freeApiKeyEnabled) auth.loadFreeKeyDefaults()
})

const idType = ref<'imdb' | 'tmdb' | 'tvdb'>('imdb')
const imageType = ref<'poster' | 'logo' | 'backdrop' | 'episode'>('poster')
const idValue = ref('tt0013442')
const lang = ref('any')
const imageSize = ref<'default' | 'small' | 'medium' | 'large' | 'verylarge'>('default')
const badgeStyle = ref('default')
const labelStyle = ref('default')
const badgeSize = ref('default')
const badgeShape = ref('default')
const badgeBackground = ref('default')
const ratingsLimit = ref('default')
const badgeDirection = ref('default')
const imagePosition = ref('default')
const imageSource = ref('default')
const textless = ref('default')
const posterSplit = ref('default')
const posterFit = ref('default')
// Backdrop-only edge insets (percent of dimension). Empty = use the server
// default; any entered value (including 0) is sent as an explicit override.
// A number input's v-model yields a number (Vue coerces `type=number`), so the
// ref holds string | number and `insetParam` normalises both.
const edgeInsetX = ref<string | number>('')
const edgeInsetY = ref<string | number>('')
const ratingsOrderList = ref<string[]>(parseRatingsOrder(DEFAULT_RATINGS_ORDER))
// Baseline the user's order is compared against to decide whether to send a
// `ratings_order` override — the server's order once loaded, else the frontend default.
const baselineOrder = ref<string[]>(parseRatingsOrder(DEFAULT_RATINGS_ORDER))
const blur = ref('default')
// Quality + main-language overlay badges. Quality style/direction are global (not
// per image type), like `lang`/`image_source`. The selected quality tiers and the
// language-code override are per-request only (no persisted setting), so they have
// no server default to reflect; `quality_style` uses the 'default' sentinel and
// falls back to the server's persisted default.
const qualityTiers = ref<string[]>([])
const qualityStyle = ref('default')
const qualityDirection = ref('default')
// The main-language icon is per-image-type (poster/logo/backdrop) on the server,
// so we keep a value per type and show/emit only the one for the currently-selected
// image type — mirroring the per-type position controls. Episodes never render the
// language badge, so the control is hidden (and no param emitted) for episode. Each
// uses the 'default' sentinel and falls back to that type's persisted server default.
const langIconByType = ref<Record<'poster' | 'logo' | 'backdrop', string>>({ poster: 'default', logo: 'default', backdrop: 'default' })
// The image types that actually render the language badge (everything but episode).
const langIconType = computed<'poster' | 'logo' | 'backdrop' | null>(() =>
  imageType.value === 'episode' ? null : imageType.value,
)
const langIcon = computed({
  get: () => (langIconType.value ? langIconByType.value[langIconType.value] : 'default'),
  set: (v: string) => { if (langIconType.value) langIconByType.value[langIconType.value] = v },
})
const langCode = ref('')
const langExclude = ref('')
// Anchor positions for the quality / main-language overlay badges. These are
// per-image-type (poster vs backdrop) on the server, so we keep a value per type
// and show/emit only the one for the currently-selected image type — mirroring
// the rating `position` control. Logo/episode ignore overlay positions, so the
// controls are hidden for them. Each uses the 'default' sentinel and falls back
// to that type's persisted server default.
const qualityPositionByType = ref<Record<'poster' | 'backdrop', string>>({ poster: 'default', backdrop: 'default' })
const langPositionByType = ref<Record<'poster' | 'backdrop', string>>({ poster: 'default', backdrop: 'default' })
// True for the image types that actually render overlay positions.
const overlayPositionType = computed<'poster' | 'backdrop' | null>(() =>
  imageType.value === 'poster' || imageType.value === 'backdrop' ? imageType.value : null,
)
const qualityPosition = computed({
  get: () => (overlayPositionType.value ? qualityPositionByType.value[overlayPositionType.value] : 'default'),
  set: (v: string) => { if (overlayPositionType.value) qualityPositionByType.value[overlayPositionType.value] = v },
})
const langPosition = computed({
  get: () => (overlayPositionType.value ? langPositionByType.value[overlayPositionType.value] : 'default'),
  set: (v: string) => { if (overlayPositionType.value) langPositionByType.value[overlayPositionType.value] = v },
})
function isQualityTier(key: string): boolean {
  return qualityTiers.value.includes(key)
}
function toggleQualityTier(key: string, checked: boolean) {
  const set = new Set(qualityTiers.value)
  if (checked) set.add(key)
  else set.delete(key)
  // Keep canonical tier order so the emitted `quality=` value is stable.
  qualityTiers.value = QUALITY_TIERS.map(t => t.key).filter(k => set.has(k))
}
const ratingsOrderChanged = computed(
  () => ratingsOrderList.value.join(',') !== baselineOrder.value.join(','),
)
const fetchError = ref('')
const fetchLoading = ref(false)
const resultUrl = ref('')
const resultImageType = ref<'poster' | 'logo' | 'backdrop' | 'episode'>('poster')

const sizeOptions = computed(() => {
  if (imageType.value === 'backdrop' || imageType.value === 'episode') {
    return [
      { value: 'default', label: 'Size: default' },
      { value: 'small', label: 'Small' },
      { value: 'medium', label: 'Medium' },
      { value: 'large', label: 'Large' },
    ]
  }
  return [
    { value: 'default', label: 'Size: default' },
    { value: 'medium', label: 'Medium' },
    { value: 'large', label: 'Large' },
    { value: 'verylarge', label: 'Very Large' },
  ]
})

// Rating sources to exclude from badges. Seeded from the server's exclusion list
// and editable via the checkboxes below; excluded sources are dimmed in the
// priority list. Sent as a `ratings_exclude` override only when changed.
const ratingsExcludeList = ref<string[]>([])
const excludeBaseline = ref<string[]>([])
const ratingsExcludeChanged = computed(
  () => ratingsExcludeList.value.join(',') !== excludeBaseline.value.join(','),
)

function isExcluded(key: string): boolean {
  return ratingsExcludeList.value.includes(key)
}
function toggleExclude(key: string, checked: boolean) {
  const set = new Set(ratingsExcludeList.value)
  if (checked) set.add(key)
  else set.delete(key)
  // Keep canonical source order so the emitted value is stable regardless of click order.
  ratingsExcludeList.value = ALL_RATING_SOURCES.map(s => s.key).filter(k => set.has(k))
}

// When the server defaults arrive, adopt the server's rating order and exclusion
// list as the new baselines. Only overwrite a list if the user hasn't edited it
// yet, so a slow fetch never clobbers an in-progress edit.
watch(defaults, (d) => {
  if (!d) return
  const serverOrder = parseRatingsOrder(d.ratings_order)
  if (!ratingsOrderChanged.value) ratingsOrderList.value = [...serverOrder]
  baselineOrder.value = serverOrder

  const serverExclude = parseRatingsExclude(d.ratings_exclude)
  if (!ratingsExcludeChanged.value) ratingsExcludeList.value = [...serverExclude]
  excludeBaseline.value = serverExclude
}, { immediate: true })

// --- Per-image-type server defaults reflected in the dropdown "default" labels ---

const typeDefaults = computed(() => {
  const d = defaults.value
  if (!d) return null
  switch (imageType.value) {
    case 'logo':
      return { badge_style: d.logo_badge_style, label_style: d.logo_label_style, badge_size: d.logo_badge_size, ratings_limit: d.logo_ratings_limit, position: null as string | null, badge_direction: null as string | null, badge_shape: d.logo_badge_shape, badge_background: d.logo_badge_background }
    case 'backdrop':
      return { badge_style: d.backdrop_badge_style, label_style: d.backdrop_label_style, badge_size: d.backdrop_badge_size, ratings_limit: d.backdrop_ratings_limit, position: d.backdrop_position, badge_direction: d.backdrop_badge_direction, badge_shape: d.backdrop_badge_shape, badge_background: d.backdrop_badge_background }
    case 'episode':
      return { badge_style: d.episode_badge_style, label_style: d.episode_label_style, badge_size: d.episode_badge_size, ratings_limit: d.episode_ratings_limit, position: d.episode_position, badge_direction: d.episode_badge_direction, badge_shape: d.episode_badge_shape, badge_background: d.episode_badge_background }
    default: // poster
      return { badge_style: d.poster_badge_style, label_style: d.poster_label_style, badge_size: d.poster_badge_size, ratings_limit: d.ratings_limit, position: d.poster_position, badge_direction: d.poster_badge_direction, badge_shape: d.poster_badge_shape, badge_background: d.poster_badge_background }
  }
})

/** Build a "<base>: default (<resolved>)" label, or plain "<base>: default" until loaded. */
function annotate(base: string, value: string | null | undefined, map: Record<string, string>): string {
  if (value == null) return `${base}: default`
  return `${base}: default (${map[value] ?? value})`
}

const langDefaultLabel = computed(() =>
  defaults.value ? `Language: any (${defaults.value.lang})` : 'Language: any',
)
const ratingsLimitDefaultLabel = computed(() => {
  const limit = typeDefaults.value?.ratings_limit
  return limit == null ? 'Max badges: default' : `Max badges: default (${limit})`
})
const badgeStyleDefaultLabel = computed(() => annotate('Badge style', typeDefaults.value?.badge_style, BADGE_STYLE_LABELS))
const labelStyleDefaultLabel = computed(() => annotate('Label style', typeDefaults.value?.label_style, LABEL_STYLE_LABELS))
const badgeSizeDefaultLabel = computed(() => annotate('Badge size', typeDefaults.value?.badge_size, BADGE_SIZE_LABELS))
const badgeShapeDefaultLabel = computed(() => annotate('Badge shape', typeDefaults.value?.badge_shape, BADGE_SHAPE_LABELS))
// Pills always render horizontally, so the badge style choice has no effect.
// Covers both an explicit pill choice and 'default' resolving to a pill server default.
const badgeShapeIsPill = computed(() =>
  badgeShape.value === 'p' || (badgeShape.value === 'default' && typeDefaults.value?.badge_shape === 'p'),
)
const badgeBackgroundDefaultLabel = computed(() => annotate('Background', typeDefaults.value?.badge_background, BADGE_BACKGROUND_LABELS))
const imageSourceDefaultLabel = computed(() => annotate('Source', defaults.value?.image_source, IMAGE_SOURCE_LABELS))
const positionDefaultLabel = computed(() => annotate('Position', typeDefaults.value?.position, POSITION_LABELS))
const badgeDirectionDefaultLabel = computed(() => annotate('Direction', typeDefaults.value?.badge_direction, BADGE_DIRECTION_LABELS))
const textlessDefaultLabel = computed(() =>
  defaults.value ? `Textless: default (${defaults.value.textless ? 'Yes' : 'No'})` : 'Textless: default',
)
const splitDefaultLabel = computed(() =>
  defaults.value ? `Split badges: default (${defaults.value.poster_badge_split ? 'Yes' : 'No'})` : 'Split badges: default',
)
const fitDefaultLabel = computed(() => annotate('Fit', defaults.value?.poster_fit, POSTER_FIT_LABELS))
const blurDefaultLabel = computed(() =>
  defaults.value ? `Blur: default (${defaults.value.episode_blur ? 'Yes' : 'No'})` : 'Blur: default',
)
const qualityStyleDefaultLabel = computed(() => annotate('Quality style', defaults.value?.quality_style, QUALITY_STYLE_LABELS))
const qualityDirectionDefaultLabel = computed(() => annotate('Quality direction', defaults.value?.quality_direction, BADGE_DIRECTION_LABELS))
// The language icon is per image type, so the "default (resolved)" label reflects
// the selected type's matching server default field (and stays plain for episode,
// which has no language badge).
const langIconServerDefault = computed(() => {
  const d = defaults.value
  if (!d || !langIconType.value) return null
  switch (langIconType.value) {
    case 'logo': return d.logo_lang_icon
    case 'backdrop': return d.backdrop_lang_icon
    default: return d.poster_lang_icon
  }
})
const langIconDefaultLabel = computed(() => annotate('Language icon', langIconServerDefault.value, LANG_ICON_LABELS))
// Overlay anchor positions are per image type, so the "default (resolved)" label
// reflects the selected type's matching server default field.
const qualityPositionServerDefault = computed(() => {
  const d = defaults.value
  if (!d || !overlayPositionType.value) return null
  return overlayPositionType.value === 'backdrop' ? d.backdrop_quality_position : d.poster_quality_position
})
const langPositionServerDefault = computed(() => {
  const d = defaults.value
  if (!d || !overlayPositionType.value) return null
  return overlayPositionType.value === 'backdrop' ? d.backdrop_lang_position : d.poster_lang_position
})
const qualityPositionDefaultLabel = computed(() => annotate('Quality position', qualityPositionServerDefault.value, POSITION_LABELS))
const langPositionDefaultLabel = computed(() => annotate('Language position', langPositionServerDefault.value, POSITION_LABELS))

// Switching image type re-applies that type's own defaults: each type carries
// its own server defaults (poster_badge_style vs logo_badge_style, etc.), so the
// per-type render controls reset to "default" and the dropdowns reflect the new
// type's settings rather than carrying over the previous type's overrides.
watch(imageType, (newType) => {
  const validValues = sizeOptions.value.map(o => o.value)
  if (!validValues.includes(imageSize.value)) {
    imageSize.value = 'default'
  }
  // Per-type render controls — reset on every switch so the form shows the
  // appropriate defaults for the newly selected image type.
  badgeStyle.value = 'default'
  labelStyle.value = 'default'
  badgeSize.value = 'default'
  ratingsLimit.value = 'default'
  badgeDirection.value = 'default'
  imagePosition.value = 'default'
  // Controls that only exist for one type; clearing them keeps the query string
  // free of params the new type would ignore.
  textless.value = 'default'
  posterSplit.value = 'default'
  posterFit.value = 'default'
  edgeInsetX.value = ''
  edgeInsetY.value = ''
  blur.value = 'default'
  // image_source is a single global default shared by every type, so it persists
  // across switches — except episode, which doesn't accept the param.
  if (newType === 'episode') {
    imageSource.value = 'default'
  }
})

const apiBase = import.meta.env.VITE_API_URL || ''

// An edge-inset field is sent as an override only when non-empty; the value is
// clamped to the server's accepted 0–50 range. Accepts the string OR number a
// number-typed v-model may produce. Returns null to omit the param.
function insetParam(raw: string | number): string | null {
  const trimmed = String(raw).trim()
  if (trimmed === '') return null
  const n = Math.round(Number(trimmed))
  if (!Number.isFinite(n)) return null
  return String(Math.min(50, Math.max(0, n)))
}

onUnmounted(() => {
  if (resultUrl.value) URL.revokeObjectURL(resultUrl.value)
})

const idPlaceholder = computed(() => {
  if (idType.value === 'imdb') return 'tt0013442'
  if (idType.value === 'tmdb') return 'movie-872585 or episode-1396-S1E1'
  return '253573'
})

const queryString = computed(() => {
  const params = new URLSearchParams()
  const langVal = lang.value === 'any' ? '' : lang.value
  if (langVal.trim()) params.set('lang', langVal.trim())
  const sizeVal = imageSize.value === 'default' ? '' : imageSize.value
  if (sizeVal) params.set('imageSize', sizeVal)
  if (ratingsOrderChanged.value) params.set('ratings_order', ratingsOrderList.value.join(','))
  // Emit even when empty, so unchecking the server's exclusions clears them.
  if (ratingsExcludeChanged.value) params.set('ratings_exclude', ratingsExcludeList.value.join(','))
  if (badgeStyle.value !== 'default') params.set('badge_style', badgeStyle.value)
  if (labelStyle.value !== 'default') params.set('label_style', labelStyle.value)
  if (badgeSize.value !== 'default') params.set('badge_size', badgeSize.value)
  if (badgeShape.value !== 'default') params.set('badge_shape', badgeShape.value)
  if (badgeBackground.value !== 'default') params.set('badge_background', badgeBackground.value)
  if (ratingsLimit.value !== 'default') params.set('ratings_limit', ratingsLimit.value)
  if (imageType.value !== 'logo' && badgeDirection.value !== 'default') params.set('badge_direction', badgeDirection.value)
  if (imageType.value !== 'logo' && imagePosition.value !== 'default') params.set('position', imagePosition.value)
  if (imageType.value !== 'episode' && imageSource.value !== 'default') params.set('image_source', imageSource.value)
  if (imageType.value === 'poster' && textless.value !== 'default') params.set('textless', textless.value)
  if (imageType.value === 'poster' && posterSplit.value !== 'default') params.set('split', posterSplit.value)
  if (imageType.value === 'poster' && posterFit.value !== 'default') params.set('fit', posterFit.value)
  if (imageType.value === 'backdrop') {
    const ex = insetParam(edgeInsetX.value)
    if (ex !== null) params.set('edge_inset_x', ex)
    const ey = insetParam(edgeInsetY.value)
    if (ey !== null) params.set('edge_inset_y', ey)
  }
  if (imageType.value === 'episode' && blur.value !== 'default') params.set('blur', blur.value)
  // Overlay badges. Quality tiers/style/direction are global; the language icon is
  // per image type. Quality tiers are stackable and sent only when at least one is
  // selected. `quality_style` is emitted only when the user overrides the server
  // default. `lang_icon` resolves to the selected type's value (and to 'default' for
  // episode, which has no language badge), so the single `?lang_icon=` maps to the
  // selected type and is emitted only when that type is overridden. `lang_code` is
  // an ISO 639-1 override that's meaningful only when a language icon is active.
  if (qualityTiers.value.length) params.set('quality', qualityTiers.value.join(','))
  if (qualityStyle.value !== 'default') params.set('quality_style', qualityStyle.value)
  // Active reflects the CURRENT image type's language icon (flag/text). Episode
  // resolves to 'default', so its language-dependent params are never emitted.
  const langIconActive = langIcon.value === 'flag' || langIcon.value === 'text'
  if (langIcon.value !== 'default') params.set('lang_icon', langIcon.value)
  const langCodeVal = langCode.value.trim()
  if (langCodeVal && langIconActive) params.set('lang_code', langCodeVal)
  const langExcludeVal = langExclude.value.trim()
  if (langExcludeVal && langIconActive) params.set('lang_exclude', langExcludeVal)
  // Anchor positions are per image type (poster vs backdrop) and meaningful only
  // when the matching overlay actually renders. `qualityPosition`/`langPosition`
  // resolve to the selected type's value (and to 'default' for logo/episode,
  // which ignore overlay positions), so a single `quality_position=`/
  // `lang_position=` is emitted for the current type only when the user overrides
  // that type's server default — mirroring the gating for `quality_style` above.
  if (qualityTiers.value.length && qualityPosition.value !== 'default')
    params.set('quality_position', qualityPosition.value)
  if (qualityTiers.value.length && qualityDirection.value !== 'default')
    params.set('quality_direction', qualityDirection.value)
  if (langIconActive && langPosition.value !== 'default')
    params.set('lang_position', langPosition.value)
  const qs = params.toString()
  return qs ? `?${qs}` : ''
})

const curlExample = computed(() => {
  const id = idValue.value.trim() || idPlaceholder.value
  const ext = imageType.value === 'logo' ? 'png' : 'jpg'
  return `curl -o ${imageType.value}.${ext} "${window.location.origin}/${FREE_API_KEY}/${idType.value}/${imageType.value}-default/${id}.${ext}${queryString.value}"`
})

const resultClass = computed(() => {
  if (resultImageType.value === 'logo') return 'max-w-[400px]'
  if (resultImageType.value === 'backdrop' || resultImageType.value === 'episode') return 'max-w-[500px] rounded-lg shadow-lg'
  return 'max-w-[200px] rounded-lg shadow-lg'
})

async function handleFetch() {
  const id = idValue.value.trim()
  if (!id) return

  fetchError.value = ''
  fetchLoading.value = true

  const prevUrl = resultUrl.value

  const ext = imageType.value === 'logo' ? 'png' : 'jpg'
  const url = `${apiBase}/${FREE_API_KEY}/${idType.value}/${imageType.value}-default/${id}.${ext}${queryString.value}`

  try {
    const res = await fetch(url)
    if (!res.ok) throw new Error(res.status === 404 ? 'Not found — check the ID and try again' : `Server error (${res.status})`)
    const blob = await res.blob()
    resultImageType.value = imageType.value
    resultUrl.value = URL.createObjectURL(blob)
    if (prevUrl) URL.revokeObjectURL(prevUrl)
  } catch (e) {
    fetchError.value = e instanceof Error && e.message ? e.message : 'Failed to fetch — check the ID and try again'
    if (prevUrl) URL.revokeObjectURL(prevUrl)
    resultUrl.value = ''
  } finally {
    fetchLoading.value = false
  }
}
</script>

<template>
  <div v-if="auth.freeApiKeyEnabled" class="rounded-lg border border-blue-500/30 bg-blue-500/5 p-4 space-y-3">
    <p class="text-sm font-medium">Free API Key Available</p>
    <p class="text-sm text-muted-foreground">
      Use the following key for poster serving (read-only, global defaults):
    </p>
    <code class="block text-sm font-mono bg-muted px-3 py-2 rounded select-all">{{ FREE_API_KEY }}</code>
    <Collapsible v-model:open="isOpen">
      <CollapsibleTrigger as-child>
        <button class="flex w-full items-center gap-2 text-sm font-medium text-muted-foreground hover:text-foreground transition-colors">
          <ChevronRight class="h-4 w-4 shrink-0 transition-transform duration-200" :class="{ 'rotate-90': isOpen }" />
          Try it out
        </button>
      </CollapsibleTrigger>
      <CollapsibleContent class="pt-3 space-y-3">
      <form class="flex flex-col gap-3" @submit.prevent="handleFetch">
        <!--
          The dynamic "default (resolved)" items are :key'd by their label so
          reka-ui re-registers the option's text when it changes — on a per-type
          switch or when the server defaults load after mount. Without this the
          closed trigger keeps the stale text reka-ui cached when the item first
          mounted (it only snapshots textContent once), so the summary shown on
          the trigger would lag the actual default for the selected image type.
        -->
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
          <Select v-model="imageSize">
            <SelectTrigger id="free-image-size" aria-label="Image size" class="bg-background">
              <SelectValue placeholder="Size: default" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem v-for="opt in sizeOptions" :key="opt.value" :value="opt.value">
                {{ opt.label }}
              </SelectItem>
            </SelectContent>
          </Select>
          <Select v-model="lang">
            <SelectTrigger id="free-lang" aria-label="Language" class="bg-background">
              <SelectValue placeholder="Language: any" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="any" :key="langDefaultLabel">{{ langDefaultLabel }}</SelectItem>
              <SelectItem v-for="l in LANGUAGES" :key="l.code" :value="l.code">
                {{ l.code }} - {{ l.name }}
              </SelectItem>
            </SelectContent>
          </Select>
          <Select v-model="ratingsLimit">
            <SelectTrigger id="free-ratings-limit" aria-label="Ratings limit" class="bg-background">
              <SelectValue placeholder="Max badges: default" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="default" :key="ratingsLimitDefaultLabel">{{ ratingsLimitDefaultLabel }}</SelectItem>
              <SelectItem v-for="n in 11" :key="n - 1" :value="String(n - 1)">
                {{ n - 1 }} {{ n - 1 === 1 ? 'badge' : 'badges' }}
              </SelectItem>
            </SelectContent>
          </Select>
          <Select v-model="badgeStyle" :disabled="badgeShapeIsPill">
            <SelectTrigger id="free-badge-style" aria-label="Badge style" class="bg-background">
              <SelectValue placeholder="Badge style: default" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="default" :key="badgeStyleDefaultLabel">{{ badgeStyleDefaultLabel }}</SelectItem>
              <SelectItem value="h">Horizontal</SelectItem>
              <SelectItem value="v">Vertical</SelectItem>
            </SelectContent>
          </Select>
          <Select v-model="labelStyle">
            <SelectTrigger id="free-label-style" aria-label="Label style" class="bg-background">
              <SelectValue placeholder="Label style: default" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="default" :key="labelStyleDefaultLabel">{{ labelStyleDefaultLabel }}</SelectItem>
              <SelectItem value="t">Text</SelectItem>
              <SelectItem value="i">Icon</SelectItem>
              <SelectItem value="o">Official</SelectItem>
            </SelectContent>
          </Select>
          <Select v-model="badgeSize">
            <SelectTrigger id="free-badge-size" aria-label="Badge size" class="bg-background">
              <SelectValue placeholder="Badge size: default" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="default" :key="badgeSizeDefaultLabel">{{ badgeSizeDefaultLabel }}</SelectItem>
              <SelectItem value="xs">Extra Small</SelectItem>
              <SelectItem value="s">Small</SelectItem>
              <SelectItem value="m">Medium</SelectItem>
              <SelectItem value="l">Large</SelectItem>
              <SelectItem value="xl">Extra Large</SelectItem>
            </SelectContent>
          </Select>
          <Select v-model="badgeShape">
            <SelectTrigger id="free-badge-shape" aria-label="Badge shape" class="bg-background">
              <SelectValue placeholder="Badge shape: default" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="default">{{ badgeShapeDefaultLabel }}</SelectItem>
              <SelectItem value="r">Rounded</SelectItem>
              <SelectItem value="p">Pill</SelectItem>
            </SelectContent>
          </Select>
          <Select v-model="badgeBackground">
            <SelectTrigger id="free-badge-background" aria-label="Badge background" class="bg-background">
              <SelectValue placeholder="Background: default" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="default">{{ badgeBackgroundDefaultLabel }}</SelectItem>
              <SelectItem value="d">Default</SelectItem>
              <SelectItem value="k">Dark</SelectItem>
              <SelectItem value="t">Transparent</SelectItem>
              <SelectItem value="n">None</SelectItem>
            </SelectContent>
          </Select>
          <Select v-if="imageType !== 'episode'" v-model="imageSource">
            <SelectTrigger id="free-image-source" aria-label="Image source" class="bg-background">
              <SelectValue placeholder="Source: default" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="default" :key="imageSourceDefaultLabel">{{ imageSourceDefaultLabel }}</SelectItem>
              <SelectItem value="t">TMDB</SelectItem>
              <SelectItem value="f">Fanart.tv</SelectItem>
            </SelectContent>
          </Select>
          <Select v-model="qualityStyle">
            <SelectTrigger id="free-quality-style" aria-label="Quality badge style" class="bg-background">
              <SelectValue placeholder="Quality style: default" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="default" :key="qualityStyleDefaultLabel">{{ qualityStyleDefaultLabel }}</SelectItem>
              <SelectItem value="text">Text</SelectItem>
              <SelectItem value="logo">Logo</SelectItem>
            </SelectContent>
          </Select>
          <Select v-model="qualityDirection">
            <SelectTrigger id="free-quality-direction" aria-label="Quality badge direction" class="bg-background">
              <SelectValue placeholder="Quality direction: default" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="default" :key="qualityDirectionDefaultLabel">{{ qualityDirectionDefaultLabel }}</SelectItem>
              <SelectItem value="h">Horizontal</SelectItem>
              <SelectItem value="v">Vertical</SelectItem>
            </SelectContent>
          </Select>
          <!-- The main-language icon is per image type (poster/logo/backdrop);
               episodes never render the language badge, so it's hidden for them.
               The v-model resolves to the selected type's value, so this single
               control feeds the right per-type `lang_icon` for the chosen type. -->
          <Select v-if="langIconType" v-model="langIcon">
            <SelectTrigger id="free-lang-icon" aria-label="Main-language icon" class="bg-background">
              <SelectValue placeholder="Language icon: default" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="default" :key="langIconDefaultLabel">{{ langIconDefaultLabel }}</SelectItem>
              <SelectItem v-for="(label, value) in LANG_ICON_LABELS" :key="value" :value="value">{{ label }}</SelectItem>
            </SelectContent>
          </Select>
          <!-- Overlay anchor positions are per image type (poster vs backdrop);
               logo/episode ignore them, so these only show for poster/backdrop.
               The v-model resolves to the selected type's value, so a single
               control feeds the right per-type `quality_position`/`lang_position`. -->
          <Select v-if="overlayPositionType" v-model="qualityPosition">
            <SelectTrigger id="free-quality-position" aria-label="Quality badge position" class="bg-background">
              <SelectValue placeholder="Quality position: default" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="default" :key="qualityPositionDefaultLabel">{{ qualityPositionDefaultLabel }}</SelectItem>
              <SelectItem value="bc">Bottom Center</SelectItem>
              <SelectItem value="tc">Top Center</SelectItem>
              <SelectItem value="l">Left</SelectItem>
              <SelectItem value="r">Right</SelectItem>
              <SelectItem value="tl">Top Left</SelectItem>
              <SelectItem value="tr">Top Right</SelectItem>
              <SelectItem value="bl">Bottom Left</SelectItem>
              <SelectItem value="br">Bottom Right</SelectItem>
            </SelectContent>
          </Select>
          <Select v-if="overlayPositionType" v-model="langPosition">
            <SelectTrigger id="free-lang-position" aria-label="Main-language badge position" class="bg-background">
              <SelectValue placeholder="Language position: default" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="default" :key="langPositionDefaultLabel">{{ langPositionDefaultLabel }}</SelectItem>
              <SelectItem value="bc">Bottom Center</SelectItem>
              <SelectItem value="tc">Top Center</SelectItem>
              <SelectItem value="l">Left</SelectItem>
              <SelectItem value="r">Right</SelectItem>
              <SelectItem value="tl">Top Left</SelectItem>
              <SelectItem value="tr">Top Right</SelectItem>
              <SelectItem value="bl">Bottom Left</SelectItem>
              <SelectItem value="br">Bottom Right</SelectItem>
            </SelectContent>
          </Select>
          <Input
            id="free-lang-code"
            v-model="langCode"
            type="text"
            placeholder="Language code (e.g. ja)"
            aria-label="Main-language ISO 639-1 override"
            class="bg-background min-w-0"
          />
          <Input
            id="free-lang-exclude"
            v-model="langExclude"
            type="text"
            placeholder="Hide language badge for (e.g. en,es)"
            aria-label="Main-language exclude ISO 639-1 codes"
            class="bg-background min-w-0"
          />
          <template v-if="imageType === 'poster'">
            <Select v-model="textless">
              <SelectTrigger id="free-textless" aria-label="Textless" class="bg-background">
                <SelectValue placeholder="Textless: default" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="default" :key="textlessDefaultLabel">{{ textlessDefaultLabel }}</SelectItem>
                <SelectItem value="true">Yes</SelectItem>
                <SelectItem value="false">No</SelectItem>
              </SelectContent>
            </Select>
            <Select v-model="posterSplit">
              <SelectTrigger id="free-split" aria-label="Split badges" class="bg-background">
                <SelectValue placeholder="Split badges: default" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="default" :key="splitDefaultLabel">{{ splitDefaultLabel }}</SelectItem>
                <SelectItem value="true">Yes</SelectItem>
                <SelectItem value="false">No</SelectItem>
              </SelectContent>
            </Select>
            <Select v-model="posterFit">
              <SelectTrigger id="free-fit" aria-label="Aspect ratio fit" class="bg-background">
                <SelectValue placeholder="Fit: default" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="default" :key="fitDefaultLabel">{{ fitDefaultLabel }}</SelectItem>
                <SelectItem value="native">Native (source ratio)</SelectItem>
                <SelectItem value="cover">Crop to 2:3</SelectItem>
                <SelectItem value="blur">Blur fill to 2:3</SelectItem>
                <SelectItem value="pad">Letterbox to 2:3</SelectItem>
              </SelectContent>
            </Select>
          </template>
          <template v-if="imageType !== 'logo'">
            <Select v-model="imagePosition">
              <SelectTrigger id="free-image-position" aria-label="Position" class="bg-background">
                <SelectValue placeholder="Position: default" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="default" :key="positionDefaultLabel">{{ positionDefaultLabel }}</SelectItem>
                <SelectItem value="bc">Bottom Center</SelectItem>
                <SelectItem value="tc">Top Center</SelectItem>
                <SelectItem value="l">Left</SelectItem>
                <SelectItem value="r">Right</SelectItem>
                <SelectItem value="tl">Top Left</SelectItem>
                <SelectItem value="tr">Top Right</SelectItem>
                <SelectItem value="bl">Bottom Left</SelectItem>
                <SelectItem value="br">Bottom Right</SelectItem>
              </SelectContent>
            </Select>
            <Select v-model="badgeDirection">
              <SelectTrigger id="free-badge-direction" aria-label="Badge direction" class="bg-background">
                <SelectValue placeholder="Direction: default" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="default" :key="badgeDirectionDefaultLabel">{{ badgeDirectionDefaultLabel }}</SelectItem>
                <SelectItem value="h">Horizontal</SelectItem>
                <SelectItem value="v">Vertical</SelectItem>
              </SelectContent>
            </Select>
          </template>
          <template v-if="imageType === 'backdrop'">
            <div class="flex items-center gap-2">
              <Label for="free-edge-inset-x" class="text-xs text-muted-foreground shrink-0 whitespace-nowrap">Horizontal inset %</Label>
              <Input
                id="free-edge-inset-x"
                v-model="edgeInsetX"
                type="number"
                min="0"
                max="50"
                :placeholder="`default (${defaults?.backdrop_edge_inset_x ?? 0})`"
                aria-label="Backdrop horizontal edge inset (percent of width, left/right positions)"
                class="bg-background min-w-0"
              />
            </div>
            <div class="flex items-center gap-2">
              <Label for="free-edge-inset-y" class="text-xs text-muted-foreground shrink-0 whitespace-nowrap">Vertical inset %</Label>
              <Input
                id="free-edge-inset-y"
                v-model="edgeInsetY"
                type="number"
                min="0"
                max="50"
                :placeholder="`default (${defaults?.backdrop_edge_inset_y ?? 0})`"
                aria-label="Backdrop vertical edge inset (percent of height, top/bottom positions)"
                class="bg-background min-w-0"
              />
            </div>
          </template>
          <template v-if="imageType === 'episode'">
            <Select v-model="blur">
              <SelectTrigger id="free-blur" aria-label="Blur" class="bg-background">
                <SelectValue placeholder="Blur: default" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="default" :key="blurDefaultLabel">{{ blurDefaultLabel }}</SelectItem>
                <SelectItem value="true">Yes</SelectItem>
                <SelectItem value="false">No</SelectItem>
              </SelectContent>
            </Select>
          </template>
        </div>
        <div class="space-y-1 flex flex-col items-center">
          <p class="text-xs text-muted-foreground">Rating priority</p>
          <RatingsOrderList v-model="ratingsOrderList" :excluded="ratingsExcludeList" compact />
        </div>
        <div class="space-y-1 flex flex-col items-center">
          <p class="text-xs text-muted-foreground">Exclude ratings</p>
          <div class="grid grid-cols-2 gap-x-3 gap-y-1.5 max-w-sm w-full">
            <div
              v-for="source in ALL_RATING_SOURCES"
              :key="source.key"
              class="flex items-start gap-1.5 text-left"
            >
              <Checkbox
                :id="`free-exclude-${source.key}`"
                :model-value="isExcluded(source.key)"
                :aria-label="`Exclude ${source.label}`"
                class="bg-background shrink-0 mt-0.5"
                @update:model-value="(v) => toggleExclude(source.key, !!v)"
              />
              <Label
                :for="`free-exclude-${source.key}`"
                class="flex items-start gap-1.5 text-xs font-normal leading-snug cursor-pointer min-w-0"
              >
                <span
                  class="inline-block w-2 h-2 rounded-full shrink-0 mt-1"
                  :style="{ backgroundColor: source.color }"
                ></span>
                <span>{{ source.label }}</span>
              </Label>
            </div>
          </div>
        </div>
        <div class="space-y-1 flex flex-col items-center">
          <p class="text-xs text-muted-foreground">Quality badges</p>
          <div class="grid grid-cols-2 gap-x-3 gap-y-1.5 max-w-sm w-full">
            <div
              v-for="tier in QUALITY_TIERS"
              :key="tier.key"
              class="flex items-center gap-1.5 text-left"
            >
              <Checkbox
                :id="`free-quality-${tier.key}`"
                :model-value="isQualityTier(tier.key)"
                :aria-label="`Quality ${tier.label}`"
                class="bg-background shrink-0"
                @update:model-value="(v) => toggleQualityTier(tier.key, !!v)"
              />
              <Label
                :for="`free-quality-${tier.key}`"
                class="text-xs font-normal leading-snug cursor-pointer min-w-0"
              >
                {{ tier.label }}
              </Label>
            </div>
          </div>
        </div>
        <code class="block text-xs font-mono bg-muted px-3 py-2 rounded text-muted-foreground break-all select-all">{{ curlExample }}</code>
        <div class="flex flex-wrap gap-2">
          <Select v-model="idType">
            <SelectTrigger id="free-id-type" aria-label="ID type" class="bg-background w-auto">
              <SelectValue placeholder="ID type" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="imdb">IMDb</SelectItem>
              <SelectItem value="tmdb">TMDb</SelectItem>
              <SelectItem value="tvdb">TVDB</SelectItem>
            </SelectContent>
          </Select>
          <Select v-model="imageType">
            <SelectTrigger id="free-image-type" aria-label="Image type" class="bg-background w-auto">
              <SelectValue placeholder="Image type" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="poster">Poster</SelectItem>
              <SelectItem value="logo">Logo</SelectItem>
              <SelectItem value="backdrop">Backdrop</SelectItem>
              <SelectItem value="episode">Episode</SelectItem>
            </SelectContent>
          </Select>
          <div class="flex gap-2 flex-1 min-w-[200px]">
            <Input
              id="free-id-value"
              v-model="idValue"
              type="text"
              :placeholder="idPlaceholder"
              required
              class="flex-1 min-w-0 font-mono bg-background"
            />
            <Button type="submit" :disabled="fetchLoading" class="shrink-0">
              <Loader2 v-if="fetchLoading" class="h-4 w-4 animate-spin" />
              <span v-else>Fetch</span>
            </Button>
          </div>
        </div>
      </form>
      <p v-if="fetchError" class="text-sm text-destructive">{{ fetchError }}</p>
      <div v-if="resultUrl" class="flex justify-center pt-2 overflow-hidden">
        <img
          :src="resultUrl"
          alt="Fetched result"
          :class="['w-full', resultClass]"
        />
      </div>
      </CollapsibleContent>
    </Collapsible>
  </div>
</template>
