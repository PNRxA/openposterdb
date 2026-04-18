<script setup lang="ts">
import { ref, computed, watch, nextTick, onBeforeUnmount } from 'vue'
import { Loader2, Check } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import RatingsOrderList from '@/components/RatingsOrderList.vue'
import type { SaveSettingsPayload } from '@/lib/api'
import { LANGUAGES, parseRatingsOrder } from '@/lib/constants'

export interface RenderSettings {
  image_source: string
  lang: string
  textless: boolean
  fanart_available: boolean
  ratings_limit: number
  ratings_order: string
  is_default?: boolean
  poster_position: string
  logo_ratings_limit: number
  backdrop_ratings_limit: number
  poster_badge_style: string
  poster_badge_background_style: string
  logo_badge_style: string
  backdrop_badge_style: string
  poster_label_style: string
  logo_label_style: string
  backdrop_label_style: string
  poster_badge_direction: string
  poster_badge_size: string
  poster_badge_scale_override: number
  logo_badge_size: string
  logo_badge_scale_override: number
  backdrop_badge_size: string
  backdrop_badge_scale_override: number
  backdrop_position: string
  backdrop_badge_direction: string
  episode_ratings_limit: number
  episode_badge_style: string
  episode_label_style: string
  episode_badge_size: string
  episode_badge_scale_override: number
  episode_position: string
  episode_badge_direction: string
  episode_blur: boolean
  poster_badge_gap: number
  poster_badge_margin: number
  logo_badge_gap: number
  logo_badge_margin: number
  backdrop_badge_gap: number
  backdrop_badge_margin: number
  episode_badge_gap: number
  episode_badge_margin: number
}

const props = defineProps<{
  settings: RenderSettings
  uid?: string
  loadSettings: () => Promise<RenderSettings | null>
  saveSettings: (s: SaveSettingsPayload) => Promise<string | null>
  resetSettings?: () => Promise<boolean>
  fetchPreview: (ratingsLimit: number, ratingsOrder: string, posterPosition?: string, badgeStyle?: string, badgeBackgroundStyle?: string, labelStyle?: string, badgeDirection?: string, badgeSize?: string, badgeScale?: number, badgeGap?: number, badgeMargin?: number) => Promise<Response>
  fetchLogoPreview?: (ratingsLimit: number, ratingsOrder: string, badgeStyle?: string, labelStyle?: string, badgeSize?: string, badgeScale?: number, badgeGap?: number, badgeMargin?: number) => Promise<Response>
  fetchBackdropPreview?: (ratingsLimit: number, ratingsOrder: string, badgeStyle?: string, labelStyle?: string, badgeSize?: string, position?: string, badgeDirection?: string, badgeScale?: number, badgeGap?: number, badgeMargin?: number) => Promise<Response>
  fetchEpisodePreview?: (ratingsLimit: number, ratingsOrder: string, badgeStyle?: string, labelStyle?: string, badgeSize?: string, position?: string, badgeDirection?: string, blur?: boolean, badgeScale?: number, badgeGap?: number, badgeMargin?: number) => Promise<Response>
}>()

const MIN_BADGE_SCALE = 0.25
const MAX_BADGE_SCALE = 4
const MIN_LAYOUT_OVERRIDE = 0
const MAX_LAYOUT_OVERRIDE = 200

function clampBadgeScale(value: unknown): number {
  const parsed = Number(value)
  if (!Number.isFinite(parsed)) return 1
  return Math.min(MAX_BADGE_SCALE, Math.max(MIN_BADGE_SCALE, parsed))
}

function clampLayoutOverride(value: unknown): number {
  const parsed = Number(value)
  if (!Number.isFinite(parsed)) return 0
  return Math.round(Math.min(MAX_LAYOUT_OVERRIDE, Math.max(MIN_LAYOUT_OVERRIDE, parsed)))
}

function isAutoLayoutOverride(value: unknown): boolean {
  const parsed = Number(value)
  return Number.isFinite(parsed) && parsed < 0
}

function fixedLayoutOverride(value: unknown): number {
  const parsed = Number(value)
  if (!Number.isFinite(parsed) || parsed < 0) return 0
  return clampLayoutOverride(parsed)
}

function effectiveLayoutOverride(auto: boolean, value: unknown): number {
  return auto ? -1 : clampLayoutOverride(value)
}

const editFanart = ref(props.settings.image_source === 'f')
const editLang = ref(props.settings.lang || 'en')
const editTextless = ref(props.settings.textless)
const editSource = computed(() => editFanart.value ? 'f' : 't')
const editRatingsLimit = ref(props.settings.ratings_limit)
const editRatingsOrder = ref<string[]>(parseRatingsOrder(props.settings.ratings_order))
const editPosterPosition = ref(props.settings.poster_position || 'bc')
const editLogoRatingsLimit = ref(props.settings.logo_ratings_limit ?? 3)
const editBackdropRatingsLimit = ref(props.settings.backdrop_ratings_limit ?? 3)
const editPosterBadgeStyle = ref(props.settings.poster_badge_style || 'd')
const editPosterBadgeBackgroundStyle = ref(props.settings.poster_badge_background_style || 'i')
const editLogoBadgeStyle = ref(props.settings.logo_badge_style || 'v')
const editBackdropBadgeStyle = ref(props.settings.backdrop_badge_style || 'v')
const editPosterLabelStyle = ref(props.settings.poster_label_style || 'o')
const editLogoLabelStyle = ref(props.settings.logo_label_style || 'o')
const editBackdropLabelStyle = ref(props.settings.backdrop_label_style || 'o')
const editPosterBadgeDirection = ref(props.settings.poster_badge_direction || 'd')
const editPosterBadgeSize = ref(props.settings.poster_badge_size || 'm')
const editPosterBadgeScaleOverride = ref(clampBadgeScale(props.settings.poster_badge_scale_override ?? 1))
const editLogoBadgeSize = ref(props.settings.logo_badge_size || 'm')
const editLogoBadgeScaleOverride = ref(clampBadgeScale(props.settings.logo_badge_scale_override ?? 1))
const editBackdropBadgeSize = ref(props.settings.backdrop_badge_size || 'm')
const editBackdropBadgeScaleOverride = ref(clampBadgeScale(props.settings.backdrop_badge_scale_override ?? 1))
const editBackdropPosition = ref(props.settings.backdrop_position || 'tr')
const editBackdropBadgeDirection = ref(props.settings.backdrop_badge_direction || 'v')
const editEpisodeRatingsLimit = ref(props.settings.episode_ratings_limit ?? 1)
const editEpisodeBadgeStyle = ref(props.settings.episode_badge_style || 'v')
const editEpisodeLabelStyle = ref(props.settings.episode_label_style || 'o')
const editEpisodeBadgeSize = ref(props.settings.episode_badge_size || 'l')
const editEpisodeBadgeScaleOverride = ref(clampBadgeScale(props.settings.episode_badge_scale_override ?? 1))
const editEpisodePosition = ref(props.settings.episode_position || 'tr')
const editEpisodeBadgeDirection = ref(props.settings.episode_badge_direction || 'v')
const editEpisodeBlur = ref(props.settings.episode_blur ?? false)
const editPosterBadgeGap = ref(clampLayoutOverride(props.settings.poster_badge_gap ?? 0))
const editPosterBadgeMargin = ref(clampLayoutOverride(props.settings.poster_badge_margin ?? 0))
const editLogoBadgeGap = ref(clampLayoutOverride(props.settings.logo_badge_gap ?? 0))
const editLogoBadgeMargin = ref(clampLayoutOverride(props.settings.logo_badge_margin ?? 0))
const editBackdropBadgeGap = ref(clampLayoutOverride(props.settings.backdrop_badge_gap ?? 0))
const editBackdropBadgeMargin = ref(clampLayoutOverride(props.settings.backdrop_badge_margin ?? 0))
const editEpisodeBadgeGap = ref(clampLayoutOverride(props.settings.episode_badge_gap ?? 0))
const editEpisodeBadgeMargin = ref(clampLayoutOverride(props.settings.episode_badge_margin ?? 0))
const editPosterBadgeGapAuto = ref(isAutoLayoutOverride(props.settings.poster_badge_gap))
const editPosterBadgeMarginAuto = ref(isAutoLayoutOverride(props.settings.poster_badge_margin))
const editLogoBadgeGapAuto = ref(isAutoLayoutOverride(props.settings.logo_badge_gap))
const editLogoBadgeMarginAuto = ref(isAutoLayoutOverride(props.settings.logo_badge_margin))
const editBackdropBadgeGapAuto = ref(isAutoLayoutOverride(props.settings.backdrop_badge_gap))
const editBackdropBadgeMarginAuto = ref(isAutoLayoutOverride(props.settings.backdrop_badge_margin))
const editEpisodeBadgeGapAuto = ref(isAutoLayoutOverride(props.settings.episode_badge_gap))
const editEpisodeBadgeMarginAuto = ref(isAutoLayoutOverride(props.settings.episode_badge_margin))

function applySettings(s: RenderSettings) {
  editFanart.value = s.image_source === 'f'
  editLang.value = s.lang || 'en'
  editTextless.value = s.textless
  editRatingsLimit.value = s.ratings_limit
  editRatingsOrder.value = parseRatingsOrder(s.ratings_order)
  editPosterPosition.value = s.poster_position || 'bc'
  editLogoRatingsLimit.value = s.logo_ratings_limit ?? 3
  editBackdropRatingsLimit.value = s.backdrop_ratings_limit ?? 3
  editPosterBadgeStyle.value = s.poster_badge_style || 'd'
  editPosterBadgeBackgroundStyle.value = s.poster_badge_background_style || 'i'
  editLogoBadgeStyle.value = s.logo_badge_style || 'v'
  editBackdropBadgeStyle.value = s.backdrop_badge_style || 'v'
  editPosterLabelStyle.value = s.poster_label_style || 'o'
  editLogoLabelStyle.value = s.logo_label_style || 'o'
  editBackdropLabelStyle.value = s.backdrop_label_style || 'o'
  editPosterBadgeDirection.value = s.poster_badge_direction || 'd'
  editPosterBadgeSize.value = s.poster_badge_size || 'm'
  editPosterBadgeScaleOverride.value = clampBadgeScale(s.poster_badge_scale_override ?? 1)
  editLogoBadgeSize.value = s.logo_badge_size || 'm'
  editLogoBadgeScaleOverride.value = clampBadgeScale(s.logo_badge_scale_override ?? 1)
  editBackdropBadgeSize.value = s.backdrop_badge_size || 'm'
  editBackdropBadgeScaleOverride.value = clampBadgeScale(s.backdrop_badge_scale_override ?? 1)
  editBackdropPosition.value = s.backdrop_position || 'tr'
  editBackdropBadgeDirection.value = s.backdrop_badge_direction || 'v'
  editEpisodeRatingsLimit.value = s.episode_ratings_limit ?? 1
  editEpisodeBadgeStyle.value = s.episode_badge_style || 'v'
  editEpisodeLabelStyle.value = s.episode_label_style || 'o'
  editEpisodeBadgeSize.value = s.episode_badge_size || 'l'
  editEpisodeBadgeScaleOverride.value = clampBadgeScale(s.episode_badge_scale_override ?? 1)
  editEpisodePosition.value = s.episode_position || 'tr'
  editEpisodeBadgeDirection.value = s.episode_badge_direction || 'v'
  editEpisodeBlur.value = s.episode_blur ?? false
  editPosterBadgeGapAuto.value = isAutoLayoutOverride(s.poster_badge_gap)
  editPosterBadgeMarginAuto.value = isAutoLayoutOverride(s.poster_badge_margin)
  editLogoBadgeGapAuto.value = isAutoLayoutOverride(s.logo_badge_gap)
  editLogoBadgeMarginAuto.value = isAutoLayoutOverride(s.logo_badge_margin)
  editBackdropBadgeGapAuto.value = isAutoLayoutOverride(s.backdrop_badge_gap)
  editBackdropBadgeMarginAuto.value = isAutoLayoutOverride(s.backdrop_badge_margin)
  editEpisodeBadgeGapAuto.value = isAutoLayoutOverride(s.episode_badge_gap)
  editEpisodeBadgeMarginAuto.value = isAutoLayoutOverride(s.episode_badge_margin)
  editPosterBadgeGap.value = fixedLayoutOverride(s.poster_badge_gap)
  editPosterBadgeMargin.value = fixedLayoutOverride(s.poster_badge_margin)
  editLogoBadgeGap.value = fixedLayoutOverride(s.logo_badge_gap)
  editLogoBadgeMargin.value = fixedLayoutOverride(s.logo_badge_margin)
  editBackdropBadgeGap.value = fixedLayoutOverride(s.backdrop_badge_gap)
  editBackdropBadgeMargin.value = fixedLayoutOverride(s.backdrop_badge_margin)
  editEpisodeBadgeGap.value = fixedLayoutOverride(s.episode_badge_gap)
  editEpisodeBadgeMargin.value = fixedLayoutOverride(s.episode_badge_margin)
}
const currentSettings = ref<RenderSettings>(props.settings)
const saving = ref(false)
const error = ref('')
const showCheck = ref(false)
let checkTimeout: ReturnType<typeof setTimeout> | null = null
let syncing = false

watch(() => props.settings, (s) => {
  syncing = true
  currentSettings.value = s
  applySettings(s)
  nextTick(() => {
    syncing = false
  })
})

function revertEdits() {
  syncing = true
  applySettings(currentSettings.value)
  nextTick(() => { syncing = false })
}

let pendingSave = false

async function autoSave() {
  if (saving.value) {
    pendingSave = true
    return
  }
  saving.value = true
  error.value = ''
  showCheck.value = false
  if (checkTimeout) clearTimeout(checkTimeout)
  try {
    const err = await props.saveSettings({
      image_source: editSource.value,
      lang: editLang.value,
      textless: editTextless.value,
      ratings_limit: editRatingsLimit.value,
      ratings_order: editRatingsOrder.value.join(','),
      poster_position: editPosterPosition.value,
      logo_ratings_limit: editLogoRatingsLimit.value,
      backdrop_ratings_limit: editBackdropRatingsLimit.value,
      poster_badge_style: editPosterBadgeStyle.value,
      poster_badge_background_style: editPosterBadgeBackgroundStyle.value,
      logo_badge_style: editLogoBadgeStyle.value,
      backdrop_badge_style: editBackdropBadgeStyle.value,
      poster_label_style: editPosterLabelStyle.value,
      logo_label_style: editLogoLabelStyle.value,
      backdrop_label_style: editBackdropLabelStyle.value,
      poster_badge_direction: editPosterBadgeDirection.value,
      poster_badge_size: editPosterBadgeSize.value,
      poster_badge_scale_override: clampBadgeScale(editPosterBadgeScaleOverride.value),
      logo_badge_size: editLogoBadgeSize.value,
      logo_badge_scale_override: clampBadgeScale(editLogoBadgeScaleOverride.value),
      backdrop_badge_size: editBackdropBadgeSize.value,
      backdrop_badge_scale_override: clampBadgeScale(editBackdropBadgeScaleOverride.value),
      backdrop_position: editBackdropPosition.value,
      backdrop_badge_direction: editBackdropBadgeDirection.value,
      episode_ratings_limit: editEpisodeRatingsLimit.value,
      episode_badge_style: editEpisodeBadgeStyle.value,
      episode_label_style: editEpisodeLabelStyle.value,
      episode_badge_size: editEpisodeBadgeSize.value,
      episode_badge_scale_override: clampBadgeScale(editEpisodeBadgeScaleOverride.value),
      episode_position: editEpisodePosition.value,
      episode_badge_direction: editEpisodeBadgeDirection.value,
      episode_blur: editEpisodeBlur.value,
      poster_badge_gap: effectiveLayoutOverride(editPosterBadgeGapAuto.value, editPosterBadgeGap.value),
      poster_badge_margin: effectiveLayoutOverride(editPosterBadgeMarginAuto.value, editPosterBadgeMargin.value),
      logo_badge_gap: effectiveLayoutOverride(editLogoBadgeGapAuto.value, editLogoBadgeGap.value),
      logo_badge_margin: effectiveLayoutOverride(editLogoBadgeMarginAuto.value, editLogoBadgeMargin.value),
      backdrop_badge_gap: effectiveLayoutOverride(editBackdropBadgeGapAuto.value, editBackdropBadgeGap.value),
      backdrop_badge_margin: effectiveLayoutOverride(editBackdropBadgeMarginAuto.value, editBackdropBadgeMargin.value),
      episode_badge_gap: effectiveLayoutOverride(editEpisodeBadgeGapAuto.value, editEpisodeBadgeGap.value),
      episode_badge_margin: effectiveLayoutOverride(editEpisodeBadgeMarginAuto.value, editEpisodeBadgeMargin.value),
    })
    if (err) {
      error.value = err
      revertEdits()
    } else {
      const updated = await props.loadSettings()
      if (updated) {
        currentSettings.value = updated
      }
      showCheck.value = true
      checkTimeout = setTimeout(() => (showCheck.value = false), 1500)
    }
  } catch {
    error.value = 'Failed to save'
    revertEdits()
  } finally {
    saving.value = false
    if (pendingSave) {
      pendingSave = false
      autoSave()
    }
  }
}

// Auto-save on any setting change
watch(
  [editSource, editLang, editTextless, editRatingsLimit, editRatingsOrder, editPosterPosition, editLogoRatingsLimit, editBackdropRatingsLimit, editPosterBadgeStyle, editPosterBadgeBackgroundStyle, editLogoBadgeStyle, editBackdropBadgeStyle, editPosterLabelStyle, editLogoLabelStyle, editBackdropLabelStyle, editPosterBadgeDirection, editPosterBadgeSize, editPosterBadgeScaleOverride, editLogoBadgeSize, editLogoBadgeScaleOverride, editBackdropBadgeSize, editBackdropBadgeScaleOverride, editBackdropPosition, editBackdropBadgeDirection, editEpisodeRatingsLimit, editEpisodeBadgeStyle, editEpisodeLabelStyle, editEpisodeBadgeSize, editEpisodeBadgeScaleOverride, editEpisodePosition, editEpisodeBadgeDirection, editEpisodeBlur, editPosterBadgeGap, editPosterBadgeMargin, editLogoBadgeGap, editLogoBadgeMargin, editBackdropBadgeGap, editBackdropBadgeMargin, editEpisodeBadgeGap, editEpisodeBadgeMargin, editPosterBadgeGapAuto, editPosterBadgeMarginAuto, editLogoBadgeGapAuto, editLogoBadgeMarginAuto, editBackdropBadgeGapAuto, editBackdropBadgeMarginAuto, editEpisodeBadgeGapAuto, editEpisodeBadgeMarginAuto],
  () => {
    if (syncing) return
    autoSave()
  },
  { deep: true },
)

async function handleReset() {
  if (!props.resetSettings) return
  saving.value = true
  error.value = ''
  showCheck.value = false
  if (checkTimeout) clearTimeout(checkTimeout)
  try {
    const ok = await props.resetSettings()
    if (ok) {
      await props.loadSettings()
      showCheck.value = true
      checkTimeout = setTimeout(() => (showCheck.value = false), 1500)
    } else {
      error.value = 'Failed to reset'
    }
  } catch {
    error.value = 'Failed to reset'
  } finally {
    saving.value = false
  }
}

// --- Preview state for poster, logo, backdrop ---
interface PreviewState {
  src: string
  loading: boolean
  error: boolean
  size: { w: number; h: number } | null
  generation: number
}

function makePreviewState(): PreviewState {
  return { src: '', loading: false, error: false, size: null, generation: 0 }
}

const posterPreview = ref<PreviewState>(makePreviewState())
const logoPreview = ref<PreviewState>(makePreviewState())
const backdropPreview = ref<PreviewState>(makePreviewState())
const episodePreview = ref<PreviewState>(makePreviewState())

function onPreviewLoad(state: PreviewState, e: Event) {
  const img = e.target as HTMLImageElement
  if (img.naturalWidth && img.naturalHeight) {
    state.size = { w: img.naturalWidth, h: img.naturalHeight }
  }
  state.loading = false
  state.error = false
}

async function fetchPreviewImage(
  state: PreviewState,
  fetcher: (ratingsLimit: number, ratingsOrder: string, posterPosition?: string, badgeStyle?: string, badgeBackgroundStyle?: string, labelStyle?: string, badgeDirection?: string, badgeSize?: string, badgeScale?: number, badgeGap?: number, badgeMargin?: number) => Promise<Response>,
  extraArgs?: { posterPosition?: string; badgeStyle?: string; badgeBackgroundStyle?: string; labelStyle?: string; badgeDirection?: string; badgeSize?: string; badgeScale?: number; badgeGap?: number; badgeMargin?: number },
) {
  state.loading = true
  state.error = false
  const generation = ++state.generation

  try {
    const res = await fetcher(editRatingsLimit.value, editRatingsOrder.value.join(','), extraArgs?.posterPosition, extraArgs?.badgeStyle, extraArgs?.badgeBackgroundStyle, extraArgs?.labelStyle, extraArgs?.badgeDirection, extraArgs?.badgeSize, extraArgs?.badgeScale, extraArgs?.badgeGap, extraArgs?.badgeMargin)
    if (generation !== state.generation) return
    if (!res.ok) {
      state.error = true
      state.loading = false
      return
    }
    const blob = await res.blob()
    if (generation !== state.generation) return
    if (state.src) URL.revokeObjectURL(state.src)
    state.src = URL.createObjectURL(blob)
  } catch {
    if (generation === state.generation) {
      state.error = true
      state.loading = false
    }
  }
}

let posterPreviewTimer: ReturnType<typeof setTimeout> | null = null
let logoPreviewTimer: ReturnType<typeof setTimeout> | null = null
let backdropPreviewTimer: ReturnType<typeof setTimeout> | null = null
let episodePreviewTimer: ReturnType<typeof setTimeout> | null = null

function updatePosterPreview() {
  fetchPreviewImage(posterPreview.value, props.fetchPreview, { posterPosition: editPosterPosition.value, badgeStyle: editPosterBadgeStyle.value, badgeBackgroundStyle: editPosterBadgeBackgroundStyle.value, labelStyle: editPosterLabelStyle.value, badgeDirection: editPosterBadgeDirection.value, badgeSize: editPosterBadgeSize.value, badgeScale: clampBadgeScale(editPosterBadgeScaleOverride.value), badgeGap: effectiveLayoutOverride(editPosterBadgeGapAuto.value, editPosterBadgeGap.value), badgeMargin: effectiveLayoutOverride(editPosterBadgeMarginAuto.value, editPosterBadgeMargin.value) })
}

function updateLogoPreview() {
  if (props.fetchLogoPreview) {
    fetchPreviewImage(logoPreview.value, (_limit, order) => props.fetchLogoPreview!(editLogoRatingsLimit.value, order, editLogoBadgeStyle.value, editLogoLabelStyle.value, editLogoBadgeSize.value, clampBadgeScale(editLogoBadgeScaleOverride.value), effectiveLayoutOverride(editLogoBadgeGapAuto.value, editLogoBadgeGap.value), effectiveLayoutOverride(editLogoBadgeMarginAuto.value, editLogoBadgeMargin.value)))
  }
}

function updateBackdropPreview() {
  if (props.fetchBackdropPreview) {
    fetchPreviewImage(backdropPreview.value, (_limit, order) => props.fetchBackdropPreview!(editBackdropRatingsLimit.value, order, editBackdropBadgeStyle.value, editBackdropLabelStyle.value, editBackdropBadgeSize.value, editBackdropPosition.value, editBackdropBadgeDirection.value, clampBadgeScale(editBackdropBadgeScaleOverride.value), effectiveLayoutOverride(editBackdropBadgeGapAuto.value, editBackdropBadgeGap.value), effectiveLayoutOverride(editBackdropBadgeMarginAuto.value, editBackdropBadgeMargin.value)))
  }
}

function updateEpisodePreview() {
  if (props.fetchEpisodePreview) {
    fetchPreviewImage(episodePreview.value, (_limit, order) => props.fetchEpisodePreview!(editEpisodeRatingsLimit.value, order, editEpisodeBadgeStyle.value, editEpisodeLabelStyle.value, editEpisodeBadgeSize.value, editEpisodePosition.value, editEpisodeBadgeDirection.value, editEpisodeBlur.value, clampBadgeScale(editEpisodeBadgeScaleOverride.value), effectiveLayoutOverride(editEpisodeBadgeGapAuto.value, editEpisodeBadgeGap.value), effectiveLayoutOverride(editEpisodeBadgeMarginAuto.value, editEpisodeBadgeMargin.value)))
  }
}

function updateAllPreviews() {
  updatePosterPreview()
  updateLogoPreview()
  updateBackdropPreview()
  updateEpisodePreview()
}

// Global settings: refresh all previews
watch([editRatingsOrder], () => {
  if (syncing) return
  if (posterPreviewTimer) clearTimeout(posterPreviewTimer)
  if (logoPreviewTimer) clearTimeout(logoPreviewTimer)
  if (backdropPreviewTimer) clearTimeout(backdropPreviewTimer)
  if (episodePreviewTimer) clearTimeout(episodePreviewTimer)
  posterPreviewTimer = setTimeout(updatePosterPreview, 500)
  logoPreviewTimer = setTimeout(updateLogoPreview, 500)
  backdropPreviewTimer = setTimeout(updateBackdropPreview, 500)
  episodePreviewTimer = setTimeout(updateEpisodePreview, 500)
}, { deep: true })

// Poster-only settings
watch([editRatingsLimit, editPosterPosition, editPosterBadgeStyle, editPosterBadgeBackgroundStyle, editPosterLabelStyle, editPosterBadgeDirection, editPosterBadgeSize, editPosterBadgeScaleOverride, editPosterBadgeGap, editPosterBadgeMargin, editPosterBadgeGapAuto, editPosterBadgeMarginAuto], () => {
  if (syncing) return
  if (posterPreviewTimer) clearTimeout(posterPreviewTimer)
  posterPreviewTimer = setTimeout(updatePosterPreview, 500)
})

// Logo-only settings
watch([editLogoRatingsLimit, editLogoBadgeStyle, editLogoLabelStyle, editLogoBadgeSize, editLogoBadgeScaleOverride, editLogoBadgeGap, editLogoBadgeMargin, editLogoBadgeGapAuto, editLogoBadgeMarginAuto], () => {
  if (syncing) return
  if (logoPreviewTimer) clearTimeout(logoPreviewTimer)
  logoPreviewTimer = setTimeout(updateLogoPreview, 500)
})

// Backdrop-only settings
watch([editBackdropRatingsLimit, editBackdropBadgeStyle, editBackdropLabelStyle, editBackdropBadgeSize, editBackdropBadgeScaleOverride, editBackdropPosition, editBackdropBadgeDirection, editBackdropBadgeGap, editBackdropBadgeMargin, editBackdropBadgeGapAuto, editBackdropBadgeMarginAuto], () => {
  if (syncing) return
  if (backdropPreviewTimer) clearTimeout(backdropPreviewTimer)
  backdropPreviewTimer = setTimeout(updateBackdropPreview, 500)
})

// Episode-only settings
watch([editEpisodeRatingsLimit, editEpisodeBadgeStyle, editEpisodeLabelStyle, editEpisodeBadgeSize, editEpisodeBadgeScaleOverride, editEpisodePosition, editEpisodeBadgeDirection, editEpisodeBlur, editEpisodeBadgeGap, editEpisodeBadgeMargin, editEpisodeBadgeGapAuto, editEpisodeBadgeMarginAuto], () => {
  if (syncing) return
  if (episodePreviewTimer) clearTimeout(episodePreviewTimer)
  episodePreviewTimer = setTimeout(updateEpisodePreview, 500)
})

// Initial preview on mount
updateAllPreviews()

onBeforeUnmount(() => {
  if (posterPreviewTimer) clearTimeout(posterPreviewTimer)
  if (logoPreviewTimer) clearTimeout(logoPreviewTimer)
  if (backdropPreviewTimer) clearTimeout(backdropPreviewTimer)
  if (episodePreviewTimer) clearTimeout(episodePreviewTimer)
  if (posterPreview.value.src) URL.revokeObjectURL(posterPreview.value.src)
  if (logoPreview.value.src) URL.revokeObjectURL(logoPreview.value.src)
  if (backdropPreview.value.src) URL.revokeObjectURL(backdropPreview.value.src)
  if (episodePreview.value.src) URL.revokeObjectURL(episodePreview.value.src)
})

const inputId = (name: string) => props.uid ? `${name}-${props.uid}` : name
</script>

<template>
  <div class="space-y-4">
    <div class="flex items-center gap-2">
      <p class="text-sm font-semibold">Image Settings</p>
      <span
        v-if="resetSettings && currentSettings.is_default"
        class="text-xs bg-secondary text-secondary-foreground px-2 py-0.5 rounded"
      >
        Using defaults
      </span>
    </div>

    <!-- Image Settings: Language (always visible) -->
    <div class="space-y-1">
      <div class="flex items-center gap-3">
        <Label :for="inputId('lang')">Language</Label>
        <Select
          :model-value="editLang"
          @update:model-value="editLang = $event as string"
        >
          <SelectTrigger :id="inputId('lang')" class="max-w-[200px]" data-testid="lang-select">
            <SelectValue placeholder="Select language" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem v-for="lang in LANGUAGES" :key="lang.code" :value="lang.code">
              {{ lang.code }} - {{ lang.name }}
            </SelectItem>
          </SelectContent>
        </Select>
      </div>
      <p class="text-xs text-muted-foreground">Best effort — falls back to English if unavailable.</p>
    </div>

    <!-- Fanart.tv preference -->
    <template v-if="currentSettings.fanart_available">
      <div class="flex items-center gap-2">
        <Checkbox
          :id="inputId('fanart')"
          :model-value="editFanart"
          data-testid="fanart-checkbox"
          @update:model-value="(v) => editFanart = !!v"
        />
        <Label :for="inputId('fanart')">Prefer Fanart.tv as image source</Label>
      </div>
    </template>

    <div class="space-y-2 pt-2">
      <p class="text-sm font-semibold">Rating Display</p>

      <div class="space-y-2">
        <Label>Rating order</Label>
        <p class="text-xs text-muted-foreground">Use the arrows to reorder. Higher items have priority.</p>
        <RatingsOrderList v-model="editRatingsOrder" />
      </div>

    </div>

    <!-- Section 2: Poster -->
    <div class="rounded-md border p-4 space-y-3">
      <p class="text-sm font-semibold">Poster</p>
      <div class="relative w-[170px]" :style="posterPreview.size ? { aspectRatio: `${posterPreview.size.w} / ${posterPreview.size.h}` } : undefined">
        <img
          v-show="posterPreview.src && !posterPreview.error"
          :src="posterPreview.src"
          alt="Poster preview"
          class="rounded border w-full"
          @load="(e: Event) => onPreviewLoad(posterPreview, e)"
          @error="posterPreview.loading = false; posterPreview.error = true"
        />
        <p v-if="posterPreview.error && !posterPreview.loading" class="text-sm text-muted-foreground py-4">Failed</p>
        <div v-if="posterPreview.loading" class="absolute inset-0 flex items-center justify-center rounded">
          <Loader2 class="size-5 animate-spin text-white drop-shadow-md" />
        </div>
      </div>
      <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 max-w-lg">
          <div class="space-y-2">
            <Label :for="inputId('poster-position')">Badge position</Label>
            <Select
              :model-value="editPosterPosition"
              @update:model-value="editPosterPosition = $event as string"
            >
              <SelectTrigger :id="inputId('poster-position')" class="max-w-xs" data-testid="poster-position-select">
                <SelectValue placeholder="Select position" />
              </SelectTrigger>
              <SelectContent>
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
          </div>
          <div class="space-y-2">
            <Label :for="inputId('poster-badge-direction')">Badge direction</Label>
            <Select
              :model-value="editPosterBadgeDirection"
              @update:model-value="editPosterBadgeDirection = $event as string"
            >
              <SelectTrigger :id="inputId('poster-badge-direction')" class="max-w-xs" data-testid="poster-badge-direction-select">
                <SelectValue placeholder="Select direction" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="d">Default</SelectItem>
                <SelectItem value="h">Horizontal</SelectItem>
                <SelectItem value="v">Vertical</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-2">
            <Label :for="inputId('poster-badge-style')">Badge style</Label>
            <Select
              :model-value="editPosterBadgeStyle"
              @update:model-value="editPosterBadgeStyle = $event as string"
            >
              <SelectTrigger :id="inputId('poster-badge-style')" class="max-w-xs" data-testid="poster-badge-style-select">
                <SelectValue placeholder="Select style" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="d">Default</SelectItem>
                <SelectItem value="h">Horizontal</SelectItem>
                <SelectItem value="v">Vertical</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-2">
            <Label :for="inputId('poster-badge-background-style')">Badge background style</Label>
            <Select
              :model-value="editPosterBadgeBackgroundStyle"
              @update:model-value="editPosterBadgeBackgroundStyle = $event as string"
            >
              <SelectTrigger :id="inputId('poster-badge-background-style')" class="max-w-xs" data-testid="poster-badge-background-style-select">
                <SelectValue placeholder="Select background style" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="i">Individual boxes</SelectItem>
                <SelectItem value="g">Grouped strip</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-2">
            <Label :for="inputId('poster-label-style')">Label style</Label>
            <Select
              :model-value="editPosterLabelStyle"
              @update:model-value="editPosterLabelStyle = $event as string"
            >
              <SelectTrigger :id="inputId('poster-label-style')" class="max-w-xs" data-testid="poster-label-style-select">
                <SelectValue placeholder="Select style" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="t">Text</SelectItem>
                <SelectItem value="i">Icon</SelectItem>
                <SelectItem value="o">Official</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-2">
            <Label :for="inputId('poster-badge-size')">Badge size</Label>
            <Select
              :model-value="editPosterBadgeSize"
              @update:model-value="editPosterBadgeSize = $event as string"
            >
              <SelectTrigger :id="inputId('poster-badge-size')" class="max-w-xs" data-testid="poster-badge-size-select">
                <SelectValue placeholder="Select size" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="xs">Extra Small</SelectItem>
                <SelectItem value="s">Small</SelectItem>
                <SelectItem value="m">Medium</SelectItem>
                <SelectItem value="l">Large</SelectItem>
                <SelectItem value="xl">Extra Large</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <Label :for="inputId('poster-badge-scale-override')">Badge scale</Label>
              <Input
                :id="inputId('poster-badge-scale-override')"
                v-model.number="editPosterBadgeScaleOverride"
                type="number"
                :min="MIN_BADGE_SCALE"
                :max="MAX_BADGE_SCALE"
                :step="0.05"
                class="w-[96px]"
                data-testid="poster-badge-scale-override-input"
              />
            </div>
            <p class="text-xs text-muted-foreground">Range: {{ MIN_BADGE_SCALE }} to {{ MAX_BADGE_SCALE }}</p>
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <Label :for="inputId('poster-badge-gap')">Badge gap</Label>
              <div class="flex items-center gap-2">
                <Checkbox
                  :id="inputId('poster-badge-gap-auto')"
                  :model-value="editPosterBadgeGapAuto"
                  data-testid="poster-badge-gap-auto-checkbox"
                  @update:model-value="(v) => editPosterBadgeGapAuto = !!v"
                />
                <Label :for="inputId('poster-badge-gap-auto')">Auto</Label>
              </div>
              <Input
                :id="inputId('poster-badge-gap')"
                v-model.number="editPosterBadgeGap"
                type="number"
                :min="MIN_LAYOUT_OVERRIDE"
                :max="MAX_LAYOUT_OVERRIDE"
                :step="1"
                :disabled="editPosterBadgeGapAuto"
                class="w-[96px]"
                data-testid="poster-badge-gap-input"
              />
            </div>
            <p class="text-xs text-muted-foreground">Auto uses scaled spacing. Disable Auto to set fixed px (0-200).</p>
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <Label :for="inputId('poster-badge-margin')">Badge margin</Label>
              <div class="flex items-center gap-2">
                <Checkbox
                  :id="inputId('poster-badge-margin-auto')"
                  :model-value="editPosterBadgeMarginAuto"
                  data-testid="poster-badge-margin-auto-checkbox"
                  @update:model-value="(v) => editPosterBadgeMarginAuto = !!v"
                />
                <Label :for="inputId('poster-badge-margin-auto')">Auto</Label>
              </div>
              <Input
                :id="inputId('poster-badge-margin')"
                v-model.number="editPosterBadgeMargin"
                type="number"
                :min="MIN_LAYOUT_OVERRIDE"
                :max="MAX_LAYOUT_OVERRIDE"
                :step="1"
                :disabled="editPosterBadgeMarginAuto"
                class="w-[96px]"
                data-testid="poster-badge-margin-input"
              />
            </div>
            <p class="text-xs text-muted-foreground">Auto uses scaled margins. Disable Auto to set fixed px (0-200).</p>
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <Label :for="inputId('ratings-limit')">Max ratings</Label>
              <Input
                :id="inputId('ratings-limit')"
                v-model.number="editRatingsLimit"
                type="number"
                :min="0"
                :max="8"
                class="w-[80px]"
                title="0 = no ratings"
              />
            </div>
            <p class="text-xs text-muted-foreground">0 = no ratings</p>
          </div>
      </div>
      <div class="flex items-center gap-2">
        <Checkbox
          :id="inputId('textless')"
          :model-value="editTextless"
          data-testid="textless-checkbox"
          @update:model-value="(v) => editTextless = !!v"
        />
        <Label :for="inputId('textless')">Prefer textless posters</Label>
      </div>
    </div>

    <!-- Section 3: Logo -->
    <div v-if="fetchLogoPreview" class="rounded-md border p-4 space-y-3">
      <p class="text-sm font-semibold">Logo</p>
      <div class="relative w-[170px]" :style="logoPreview.size ? { aspectRatio: `${logoPreview.size.w} / ${logoPreview.size.h}` } : undefined">
        <img
          v-show="logoPreview.src && !logoPreview.error"
          :src="logoPreview.src"
          alt="Logo preview"
          class="rounded border w-full bg-neutral-900"
          @load="(e: Event) => onPreviewLoad(logoPreview, e)"
          @error="logoPreview.loading = false; logoPreview.error = true"
        />
        <p v-if="logoPreview.error && !logoPreview.loading" class="text-sm text-muted-foreground py-4">Failed</p>
        <div v-if="logoPreview.loading" class="absolute inset-0 flex items-center justify-center rounded">
          <Loader2 class="size-5 animate-spin text-white drop-shadow-md" />
        </div>
      </div>
      <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 max-w-lg">
          <div class="space-y-2">
            <Label :for="inputId('logo-badge-style')">Badge style</Label>
            <Select
              :model-value="editLogoBadgeStyle"
              @update:model-value="editLogoBadgeStyle = $event as string"
            >
              <SelectTrigger :id="inputId('logo-badge-style')" class="max-w-xs" data-testid="logo-badge-style-select">
                <SelectValue placeholder="Select style" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="h">Horizontal</SelectItem>
                <SelectItem value="v">Vertical</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-2">
            <Label :for="inputId('logo-label-style')">Label style</Label>
            <Select
              :model-value="editLogoLabelStyle"
              @update:model-value="editLogoLabelStyle = $event as string"
            >
              <SelectTrigger :id="inputId('logo-label-style')" class="max-w-xs" data-testid="logo-label-style-select">
                <SelectValue placeholder="Select style" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="t">Text</SelectItem>
                <SelectItem value="i">Icon</SelectItem>
                <SelectItem value="o">Official</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-2">
            <Label :for="inputId('logo-badge-size')">Badge size</Label>
            <Select
              :model-value="editLogoBadgeSize"
              @update:model-value="editLogoBadgeSize = $event as string"
            >
              <SelectTrigger :id="inputId('logo-badge-size')" class="max-w-xs" data-testid="logo-badge-size-select">
                <SelectValue placeholder="Select size" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="xs">Extra Small</SelectItem>
                <SelectItem value="s">Small</SelectItem>
                <SelectItem value="m">Medium</SelectItem>
                <SelectItem value="l">Large</SelectItem>
                <SelectItem value="xl">Extra Large</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <Label :for="inputId('logo-badge-scale-override')">Badge scale</Label>
              <Input
                :id="inputId('logo-badge-scale-override')"
                v-model.number="editLogoBadgeScaleOverride"
                type="number"
                :min="MIN_BADGE_SCALE"
                :max="MAX_BADGE_SCALE"
                :step="0.05"
                class="w-[96px]"
                data-testid="logo-badge-scale-override-input"
              />
            </div>
            <p class="text-xs text-muted-foreground">Range: {{ MIN_BADGE_SCALE }} to {{ MAX_BADGE_SCALE }}</p>
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <Label :for="inputId('logo-badge-gap')">Badge gap</Label>
              <div class="flex items-center gap-2">
                <Checkbox
                  :id="inputId('logo-badge-gap-auto')"
                  :model-value="editLogoBadgeGapAuto"
                  data-testid="logo-badge-gap-auto-checkbox"
                  @update:model-value="(v) => editLogoBadgeGapAuto = !!v"
                />
                <Label :for="inputId('logo-badge-gap-auto')">Auto</Label>
              </div>
              <Input
                :id="inputId('logo-badge-gap')"
                v-model.number="editLogoBadgeGap"
                type="number"
                :min="MIN_LAYOUT_OVERRIDE"
                :max="MAX_LAYOUT_OVERRIDE"
                :step="1"
                :disabled="editLogoBadgeGapAuto"
                class="w-[96px]"
                data-testid="logo-badge-gap-input"
              />
            </div>
            <p class="text-xs text-muted-foreground">Auto uses scaled spacing. Disable Auto to set fixed px (0-200).</p>
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <Label :for="inputId('logo-badge-margin')">Badge margin</Label>
              <div class="flex items-center gap-2">
                <Checkbox
                  :id="inputId('logo-badge-margin-auto')"
                  :model-value="editLogoBadgeMarginAuto"
                  data-testid="logo-badge-margin-auto-checkbox"
                  @update:model-value="(v) => editLogoBadgeMarginAuto = !!v"
                />
                <Label :for="inputId('logo-badge-margin-auto')">Auto</Label>
              </div>
              <Input
                :id="inputId('logo-badge-margin')"
                v-model.number="editLogoBadgeMargin"
                type="number"
                :min="MIN_LAYOUT_OVERRIDE"
                :max="MAX_LAYOUT_OVERRIDE"
                :step="1"
                :disabled="editLogoBadgeMarginAuto"
                class="w-[96px]"
                data-testid="logo-badge-margin-input"
              />
            </div>
            <p class="text-xs text-muted-foreground">Auto uses scaled margins. Disable Auto to set fixed px (0-200).</p>
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <Label :for="inputId('logo-ratings-limit')">Max ratings</Label>
              <Input
                :id="inputId('logo-ratings-limit')"
                v-model.number="editLogoRatingsLimit"
                type="number"
                :min="0"
                :max="8"
                class="w-[80px]"
                title="0 = no ratings"
              />
            </div>
            <p class="text-xs text-muted-foreground">0 = no ratings</p>
          </div>
      </div>
    </div>

    <!-- Section 4: Backdrop -->
    <div v-if="fetchBackdropPreview" class="rounded-md border p-4 space-y-3">
      <p class="text-sm font-semibold">Backdrop (series/movie)</p>
      <div class="relative w-[280px]" :style="backdropPreview.size ? { aspectRatio: `${backdropPreview.size.w} / ${backdropPreview.size.h}` } : undefined">
        <img
          v-show="backdropPreview.src && !backdropPreview.error"
          :src="backdropPreview.src"
          alt="Backdrop preview"
          class="rounded border w-full"
          @load="(e: Event) => onPreviewLoad(backdropPreview, e)"
          @error="backdropPreview.loading = false; backdropPreview.error = true"
        />
        <p v-if="backdropPreview.error && !backdropPreview.loading" class="text-sm text-muted-foreground py-4">Failed</p>
        <div v-if="backdropPreview.loading" class="absolute inset-0 flex items-center justify-center rounded">
          <Loader2 class="size-5 animate-spin text-white drop-shadow-md" />
        </div>
      </div>
      <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 max-w-lg">
          <div class="space-y-2">
            <Label :for="inputId('backdrop-badge-style')">Badge style</Label>
            <Select
              :model-value="editBackdropBadgeStyle"
              @update:model-value="editBackdropBadgeStyle = $event as string"
            >
              <SelectTrigger :id="inputId('backdrop-badge-style')" class="max-w-xs" data-testid="backdrop-badge-style-select">
                <SelectValue placeholder="Select style" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="h">Horizontal</SelectItem>
                <SelectItem value="v">Vertical</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-2">
            <Label :for="inputId('backdrop-label-style')">Label style</Label>
            <Select
              :model-value="editBackdropLabelStyle"
              @update:model-value="editBackdropLabelStyle = $event as string"
            >
              <SelectTrigger :id="inputId('backdrop-label-style')" class="max-w-xs" data-testid="backdrop-label-style-select">
                <SelectValue placeholder="Select style" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="t">Text</SelectItem>
                <SelectItem value="i">Icon</SelectItem>
                <SelectItem value="o">Official</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-2">
            <Label :for="inputId('backdrop-badge-size')">Badge size</Label>
            <Select
              :model-value="editBackdropBadgeSize"
              @update:model-value="editBackdropBadgeSize = $event as string"
            >
              <SelectTrigger :id="inputId('backdrop-badge-size')" class="max-w-xs" data-testid="backdrop-badge-size-select">
                <SelectValue placeholder="Select size" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="xs">Extra Small</SelectItem>
                <SelectItem value="s">Small</SelectItem>
                <SelectItem value="m">Medium</SelectItem>
                <SelectItem value="l">Large</SelectItem>
                <SelectItem value="xl">Extra Large</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-2">
            <Label :for="inputId('backdrop-position')">Position</Label>
            <Select
              :model-value="editBackdropPosition"
              @update:model-value="editBackdropPosition = $event as string"
            >
              <SelectTrigger :id="inputId('backdrop-position')" class="max-w-xs" data-testid="backdrop-position-select">
                <SelectValue placeholder="Select position" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="tl">Top Left</SelectItem>
                <SelectItem value="tc">Top Center</SelectItem>
                <SelectItem value="tr">Top Right</SelectItem>
                <SelectItem value="bl">Bottom Left</SelectItem>
                <SelectItem value="bc">Bottom Center</SelectItem>
                <SelectItem value="br">Bottom Right</SelectItem>
                <SelectItem value="l">Left</SelectItem>
                <SelectItem value="r">Right</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-2">
            <Label :for="inputId('backdrop-badge-direction')">Badge direction</Label>
            <Select
              :model-value="editBackdropBadgeDirection"
              @update:model-value="editBackdropBadgeDirection = $event as string"
            >
              <SelectTrigger :id="inputId('backdrop-badge-direction')" class="max-w-xs" data-testid="backdrop-badge-direction-select">
                <SelectValue placeholder="Select direction" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="d">Default</SelectItem>
                <SelectItem value="h">Horizontal</SelectItem>
                <SelectItem value="v">Vertical</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <Label :for="inputId('backdrop-badge-scale-override')">Badge scale</Label>
              <Input
                :id="inputId('backdrop-badge-scale-override')"
                v-model.number="editBackdropBadgeScaleOverride"
                type="number"
                :min="MIN_BADGE_SCALE"
                :max="MAX_BADGE_SCALE"
                :step="0.05"
                class="w-[96px]"
                data-testid="backdrop-badge-scale-override-input"
              />
            </div>
            <p class="text-xs text-muted-foreground">Range: {{ MIN_BADGE_SCALE }} to {{ MAX_BADGE_SCALE }}</p>
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <Label :for="inputId('backdrop-badge-gap')">Badge gap</Label>
              <div class="flex items-center gap-2">
                <Checkbox
                  :id="inputId('backdrop-badge-gap-auto')"
                  :model-value="editBackdropBadgeGapAuto"
                  data-testid="backdrop-badge-gap-auto-checkbox"
                  @update:model-value="(v) => editBackdropBadgeGapAuto = !!v"
                />
                <Label :for="inputId('backdrop-badge-gap-auto')">Auto</Label>
              </div>
              <Input
                :id="inputId('backdrop-badge-gap')"
                v-model.number="editBackdropBadgeGap"
                type="number"
                :min="MIN_LAYOUT_OVERRIDE"
                :max="MAX_LAYOUT_OVERRIDE"
                :step="1"
                :disabled="editBackdropBadgeGapAuto"
                class="w-[96px]"
                data-testid="backdrop-badge-gap-input"
              />
            </div>
            <p class="text-xs text-muted-foreground">Auto uses scaled spacing. Disable Auto to set fixed px (0-200).</p>
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <Label :for="inputId('backdrop-badge-margin')">Badge margin</Label>
              <div class="flex items-center gap-2">
                <Checkbox
                  :id="inputId('backdrop-badge-margin-auto')"
                  :model-value="editBackdropBadgeMarginAuto"
                  data-testid="backdrop-badge-margin-auto-checkbox"
                  @update:model-value="(v) => editBackdropBadgeMarginAuto = !!v"
                />
                <Label :for="inputId('backdrop-badge-margin-auto')">Auto</Label>
              </div>
              <Input
                :id="inputId('backdrop-badge-margin')"
                v-model.number="editBackdropBadgeMargin"
                type="number"
                :min="MIN_LAYOUT_OVERRIDE"
                :max="MAX_LAYOUT_OVERRIDE"
                :step="1"
                :disabled="editBackdropBadgeMarginAuto"
                class="w-[96px]"
                data-testid="backdrop-badge-margin-input"
              />
            </div>
            <p class="text-xs text-muted-foreground">Auto uses scaled margins. Disable Auto to set fixed px (0-200).</p>
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <Label :for="inputId('backdrop-ratings-limit')">Max ratings</Label>
              <Input
                :id="inputId('backdrop-ratings-limit')"
                v-model.number="editBackdropRatingsLimit"
                type="number"
                :min="0"
                :max="8"
                class="w-[80px]"
                title="0 = no ratings"
              />
            </div>
            <p class="text-xs text-muted-foreground">0 = no ratings</p>
          </div>
      </div>
    </div>

    <!-- Section 5: Episode -->
    <div v-if="fetchEpisodePreview" class="rounded-md border p-4 space-y-3">
      <p class="text-sm font-semibold">Episode</p>
      <div class="relative w-[280px]" :style="episodePreview.size ? { aspectRatio: `${episodePreview.size.w} / ${episodePreview.size.h}` } : undefined">
        <img
          v-show="episodePreview.src && !episodePreview.error"
          :src="episodePreview.src"
          alt="Episode preview"
          class="rounded border w-full"
          @load="(e: Event) => onPreviewLoad(episodePreview, e)"
          @error="episodePreview.loading = false; episodePreview.error = true"
        />
        <p v-if="episodePreview.error && !episodePreview.loading" class="text-sm text-muted-foreground py-4">Failed</p>
        <div v-if="episodePreview.loading" class="absolute inset-0 flex items-center justify-center rounded">
          <Loader2 class="size-5 animate-spin text-white drop-shadow-md" />
        </div>
      </div>
      <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 max-w-lg">
          <div class="space-y-2">
            <Label :for="inputId('episode-position')">Position</Label>
            <Select
              :model-value="editEpisodePosition"
              @update:model-value="editEpisodePosition = $event as string"
            >
              <SelectTrigger :id="inputId('episode-position')" class="max-w-xs" data-testid="episode-position-select">
                <SelectValue placeholder="Select position" />
              </SelectTrigger>
              <SelectContent>
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
          </div>
          <div class="space-y-2">
            <Label :for="inputId('episode-badge-direction')">Direction</Label>
            <Select
              :model-value="editEpisodeBadgeDirection"
              @update:model-value="editEpisodeBadgeDirection = $event as string"
            >
              <SelectTrigger :id="inputId('episode-badge-direction')" class="max-w-xs" data-testid="episode-badge-direction-select">
                <SelectValue placeholder="Select direction" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="d">Auto</SelectItem>
                <SelectItem value="h">Horizontal</SelectItem>
                <SelectItem value="v">Vertical</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-2">
            <Label :for="inputId('episode-badge-style')">Badge style</Label>
            <Select
              :model-value="editEpisodeBadgeStyle"
              @update:model-value="editEpisodeBadgeStyle = $event as string"
            >
              <SelectTrigger :id="inputId('episode-badge-style')" class="max-w-xs" data-testid="episode-badge-style-select">
                <SelectValue placeholder="Select style" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="h">Horizontal</SelectItem>
                <SelectItem value="v">Vertical</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-2">
            <Label :for="inputId('episode-label-style')">Label style</Label>
            <Select
              :model-value="editEpisodeLabelStyle"
              @update:model-value="editEpisodeLabelStyle = $event as string"
            >
              <SelectTrigger :id="inputId('episode-label-style')" class="max-w-xs" data-testid="episode-label-style-select">
                <SelectValue placeholder="Select style" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="t">Text</SelectItem>
                <SelectItem value="i">Icon</SelectItem>
                <SelectItem value="o">Official</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-2">
            <Label :for="inputId('episode-badge-size')">Badge size</Label>
            <Select
              :model-value="editEpisodeBadgeSize"
              @update:model-value="editEpisodeBadgeSize = $event as string"
            >
              <SelectTrigger :id="inputId('episode-badge-size')" class="max-w-xs" data-testid="episode-badge-size-select">
                <SelectValue placeholder="Select size" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="xs">Extra Small</SelectItem>
                <SelectItem value="s">Small</SelectItem>
                <SelectItem value="m">Medium</SelectItem>
                <SelectItem value="l">Large</SelectItem>
                <SelectItem value="xl">Extra Large</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <Label :for="inputId('episode-badge-scale-override')">Badge scale</Label>
              <Input
                :id="inputId('episode-badge-scale-override')"
                v-model.number="editEpisodeBadgeScaleOverride"
                type="number"
                :min="MIN_BADGE_SCALE"
                :max="MAX_BADGE_SCALE"
                :step="0.05"
                class="w-[96px]"
                data-testid="episode-badge-scale-override-input"
              />
            </div>
            <p class="text-xs text-muted-foreground">Range: {{ MIN_BADGE_SCALE }} to {{ MAX_BADGE_SCALE }}</p>
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <Label :for="inputId('episode-badge-gap')">Badge gap</Label>
              <div class="flex items-center gap-2">
                <Checkbox
                  :id="inputId('episode-badge-gap-auto')"
                  :model-value="editEpisodeBadgeGapAuto"
                  data-testid="episode-badge-gap-auto-checkbox"
                  @update:model-value="(v) => editEpisodeBadgeGapAuto = !!v"
                />
                <Label :for="inputId('episode-badge-gap-auto')">Auto</Label>
              </div>
              <Input
                :id="inputId('episode-badge-gap')"
                v-model.number="editEpisodeBadgeGap"
                type="number"
                :min="MIN_LAYOUT_OVERRIDE"
                :max="MAX_LAYOUT_OVERRIDE"
                :step="1"
                :disabled="editEpisodeBadgeGapAuto"
                class="w-[96px]"
                data-testid="episode-badge-gap-input"
              />
            </div>
            <p class="text-xs text-muted-foreground">Auto uses scaled spacing. Disable Auto to set fixed px (0-200).</p>
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <Label :for="inputId('episode-badge-margin')">Badge margin</Label>
              <div class="flex items-center gap-2">
                <Checkbox
                  :id="inputId('episode-badge-margin-auto')"
                  :model-value="editEpisodeBadgeMarginAuto"
                  data-testid="episode-badge-margin-auto-checkbox"
                  @update:model-value="(v) => editEpisodeBadgeMarginAuto = !!v"
                />
                <Label :for="inputId('episode-badge-margin-auto')">Auto</Label>
              </div>
              <Input
                :id="inputId('episode-badge-margin')"
                v-model.number="editEpisodeBadgeMargin"
                type="number"
                :min="MIN_LAYOUT_OVERRIDE"
                :max="MAX_LAYOUT_OVERRIDE"
                :step="1"
                :disabled="editEpisodeBadgeMarginAuto"
                class="w-[96px]"
                data-testid="episode-badge-margin-input"
              />
            </div>
            <p class="text-xs text-muted-foreground">Auto uses scaled margins. Disable Auto to set fixed px (0-200).</p>
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <Label :for="inputId('episode-ratings-limit')">Max ratings</Label>
              <Input
                :id="inputId('episode-ratings-limit')"
                v-model.number="editEpisodeRatingsLimit"
                type="number"
                :min="0"
                :max="8"
                class="w-[80px]"
                title="0 = no ratings"
              />
            </div>
            <p class="text-xs text-muted-foreground">0 = no ratings</p>
          </div>
          <div class="flex items-center gap-2 col-span-full">
            <Checkbox
              :id="inputId('episode-blur')"
              :model-value="editEpisodeBlur"
              data-testid="episode-blur-checkbox"
              @update:model-value="(v) => editEpisodeBlur = !!v"
            />
            <Label :for="inputId('episode-blur')">Blur (spoiler protection)</Label>
          </div>
      </div>
    </div>

    <div class="flex items-center gap-3 pt-1 min-h-[32px]">
      <Button
        v-if="resetSettings && !currentSettings.is_default"
        variant="outline"
        size="sm"
        :disabled="saving"
        @click="handleReset"
      >
        Reset to defaults
      </Button>
      <Transition
        enter-active-class="transition duration-200 ease-out"
        enter-from-class="opacity-0"
        enter-to-class="opacity-100"
        leave-active-class="transition duration-150 ease-in"
        leave-from-class="opacity-100"
        leave-to-class="opacity-0"
      >
        <span v-if="saving" class="flex items-center gap-1.5 text-sm text-muted-foreground">
          <Loader2 class="size-4 animate-spin" />
          Saving...
        </span>
        <span v-else-if="showCheck" class="flex items-center gap-1.5 text-sm text-green-500">
          <Check class="size-4" />
          Saved
        </span>
      </Transition>
      <span v-if="error" class="text-sm text-destructive">{{ error }}</span>
    </div>
  </div>
</template>
