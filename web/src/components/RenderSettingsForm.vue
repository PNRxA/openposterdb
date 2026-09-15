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
import { LANGUAGES, ALL_RATING_SOURCES, parseRatingsOrder, parseRatingsExclude } from '@/lib/constants'

export interface RenderSettings {
  image_source: string
  lang: string
  textless: boolean
  fanart_available: boolean
  ratings_limit: number
  ratings_order: string
  ratings_exclude: string
  is_default?: boolean
  poster_position: string
  logo_ratings_limit: number
  backdrop_ratings_limit: number
  poster_badge_style: string
  logo_badge_style: string
  backdrop_badge_style: string
  poster_label_style: string
  logo_label_style: string
  backdrop_label_style: string
  poster_badge_direction: string
  poster_badge_split: boolean
  poster_fit: string
  poster_badge_size: string
  logo_badge_size: string
  backdrop_badge_size: string
  backdrop_position: string
  backdrop_badge_direction: string
  backdrop_edge_inset_x: number
  backdrop_edge_inset_y: number
  episode_ratings_limit: number
  episode_badge_style: string
  episode_label_style: string
  episode_badge_size: string
  episode_position: string
  episode_badge_direction: string
  episode_blur: boolean
  season_ratings_limit: number
  season_badge_style: string
  season_label_style: string
  season_badge_size: string
  season_position: string
  season_badge_direction: string
  poster_badge_shape: string
  logo_badge_shape: string
  backdrop_badge_shape: string
  episode_badge_shape: string
  season_badge_shape: string
  poster_badge_background: string
  logo_badge_background: string
  backdrop_badge_background: string
  episode_badge_background: string
  season_badge_background: string
}

const props = defineProps<{
  settings: RenderSettings
  uid?: string
  loadSettings: () => Promise<RenderSettings | null>
  saveSettings: (s: SaveSettingsPayload) => Promise<string | null>
  resetSettings?: () => Promise<boolean>
  fetchPreview: (ratingsLimit: number, ratingsOrder: string, posterPosition?: string, badgeStyle?: string, labelStyle?: string, badgeDirection?: string, badgeSize?: string, ratingsExclude?: string, posterSplit?: boolean, badgeShape?: string, badgeBackground?: string, posterFit?: string) => Promise<Response>
  fetchLogoPreview?: (ratingsLimit: number, ratingsOrder: string, badgeStyle?: string, labelStyle?: string, badgeSize?: string, ratingsExclude?: string, badgeShape?: string, badgeBackground?: string) => Promise<Response>
  fetchBackdropPreview?: (ratingsLimit: number, ratingsOrder: string, badgeStyle?: string, labelStyle?: string, badgeSize?: string, position?: string, badgeDirection?: string, ratingsExclude?: string, badgeShape?: string, badgeBackground?: string, edgeInsetX?: number, edgeInsetY?: number) => Promise<Response>
  fetchEpisodePreview?: (ratingsLimit: number, ratingsOrder: string, badgeStyle?: string, labelStyle?: string, badgeSize?: string, position?: string, badgeDirection?: string, blur?: boolean, ratingsExclude?: string, badgeShape?: string, badgeBackground?: string) => Promise<Response>
  fetchSeasonPreview?: (ratingsLimit: number, ratingsOrder: string, badgeStyle?: string, labelStyle?: string, badgeSize?: string, position?: string, badgeDirection?: string, ratingsExclude?: string, badgeShape?: string, badgeBackground?: string) => Promise<Response>
}>()

const editFanart = ref(props.settings.image_source === 'f')
const editLang = ref(props.settings.lang || 'en')
const editTextless = ref(props.settings.textless)
const editSource = computed(() => editFanart.value ? 'f' : 't')
const editRatingsLimit = ref(props.settings.ratings_limit)
const editRatingsOrder = ref<string[]>(parseRatingsOrder(props.settings.ratings_order))
// Excluded sources are stored as only the explicitly-checked keys (unlike order,
// which is normalised to include every source).
const editRatingsExclude = ref<string[]>(parseRatingsExclude(props.settings.ratings_exclude))
const editPosterPosition = ref(props.settings.poster_position || 'bc')
const editLogoRatingsLimit = ref(props.settings.logo_ratings_limit ?? 3)
const editBackdropRatingsLimit = ref(props.settings.backdrop_ratings_limit ?? 3)
const editPosterBadgeStyle = ref(props.settings.poster_badge_style || 'd')
const editLogoBadgeStyle = ref(props.settings.logo_badge_style || 'v')
const editBackdropBadgeStyle = ref(props.settings.backdrop_badge_style || 'v')
const editPosterLabelStyle = ref(props.settings.poster_label_style || 'o')
const editLogoLabelStyle = ref(props.settings.logo_label_style || 'o')
const editBackdropLabelStyle = ref(props.settings.backdrop_label_style || 'o')
const editPosterBadgeDirection = ref(props.settings.poster_badge_direction || 'd')
const editPosterBadgeSplit = ref(props.settings.poster_badge_split ?? false)
const editPosterFit = ref(props.settings.poster_fit || 'native')
const editPosterBadgeSize = ref(props.settings.poster_badge_size || 'm')
const editLogoBadgeSize = ref(props.settings.logo_badge_size || 'm')
const editBackdropBadgeSize = ref(props.settings.backdrop_badge_size || 'm')
const editBackdropPosition = ref(props.settings.backdrop_position || 'tr')
const editBackdropBadgeDirection = ref(props.settings.backdrop_badge_direction || 'd')
const editBackdropEdgeInsetX = ref(props.settings.backdrop_edge_inset_x ?? 0)
const editBackdropEdgeInsetY = ref(props.settings.backdrop_edge_inset_y ?? 0)
const editEpisodeRatingsLimit = ref(props.settings.episode_ratings_limit ?? 1)
const editEpisodeBadgeStyle = ref(props.settings.episode_badge_style || 'v')
const editEpisodeLabelStyle = ref(props.settings.episode_label_style || 'o')
const editEpisodeBadgeSize = ref(props.settings.episode_badge_size || 'l')
const editEpisodePosition = ref(props.settings.episode_position || 'tr')
const editEpisodeBadgeDirection = ref(props.settings.episode_badge_direction || 'v')
const editEpisodeBlur = ref(props.settings.episode_blur ?? false)
const editSeasonRatingsLimit = ref(props.settings.season_ratings_limit ?? 3)
const editSeasonBadgeStyle = ref(props.settings.season_badge_style || 'd')
const editSeasonLabelStyle = ref(props.settings.season_label_style || 'o')
const editSeasonBadgeSize = ref(props.settings.season_badge_size || 'm')
const editSeasonPosition = ref(props.settings.season_position || 'bc')
const editSeasonBadgeDirection = ref(props.settings.season_badge_direction || 'd')
const editPosterBadgeShape = ref(props.settings.poster_badge_shape || 'r')
const editLogoBadgeShape = ref(props.settings.logo_badge_shape || 'r')
const editBackdropBadgeShape = ref(props.settings.backdrop_badge_shape || 'r')
const editEpisodeBadgeShape = ref(props.settings.episode_badge_shape || 'r')
const editSeasonBadgeShape = ref(props.settings.season_badge_shape || 'r')
const editPosterBadgeBackground = ref(props.settings.poster_badge_background || 'd')
const editLogoBadgeBackground = ref(props.settings.logo_badge_background || 'd')
const editBackdropBadgeBackground = ref(props.settings.backdrop_badge_background || 'd')
const editEpisodeBadgeBackground = ref(props.settings.episode_badge_background || 'd')
const editSeasonBadgeBackground = ref(props.settings.season_badge_background || 'd')

// Which edges the current backdrop position anchors to. The horizontal inset
// only applies to left/right positions and the vertical inset only to top/bottom
// positions; centered axes hide their control (and the server ignores them).
const backdropIsTop = computed(() => ['tl', 'tc', 'tr'].includes(editBackdropPosition.value))
const backdropIsBottom = computed(() => ['bl', 'bc', 'br'].includes(editBackdropPosition.value))
const backdropIsLeft = computed(() => ['tl', 'bl', 'l'].includes(editBackdropPosition.value))
const backdropIsRight = computed(() => ['tr', 'br', 'r'].includes(editBackdropPosition.value))
const backdropShowVerticalInset = computed(() => backdropIsTop.value || backdropIsBottom.value)
const backdropShowHorizontalInset = computed(() => backdropIsLeft.value || backdropIsRight.value)
const backdropVerticalInsetLabel = computed(() => (backdropIsTop.value ? 'Space from top' : 'Space from bottom'))
const backdropHorizontalInsetLabel = computed(() => (backdropIsLeft.value ? 'Space from left' : 'Space from right'))

function applySettings(s: RenderSettings) {
  editFanart.value = s.image_source === 'f'
  editLang.value = s.lang || 'en'
  editTextless.value = s.textless
  editRatingsLimit.value = s.ratings_limit
  editRatingsOrder.value = parseRatingsOrder(s.ratings_order)
  editRatingsExclude.value = parseRatingsExclude(s.ratings_exclude)
  editPosterPosition.value = s.poster_position || 'bc'
  editLogoRatingsLimit.value = s.logo_ratings_limit ?? 3
  editBackdropRatingsLimit.value = s.backdrop_ratings_limit ?? 3
  editPosterBadgeStyle.value = s.poster_badge_style || 'd'
  editLogoBadgeStyle.value = s.logo_badge_style || 'v'
  editBackdropBadgeStyle.value = s.backdrop_badge_style || 'v'
  editPosterLabelStyle.value = s.poster_label_style || 'o'
  editLogoLabelStyle.value = s.logo_label_style || 'o'
  editBackdropLabelStyle.value = s.backdrop_label_style || 'o'
  editPosterBadgeDirection.value = s.poster_badge_direction || 'd'
  editPosterBadgeSplit.value = s.poster_badge_split ?? false
  editPosterFit.value = s.poster_fit || 'native'
  editPosterBadgeSize.value = s.poster_badge_size || 'm'
  editLogoBadgeSize.value = s.logo_badge_size || 'm'
  editBackdropBadgeSize.value = s.backdrop_badge_size || 'm'
  editBackdropPosition.value = s.backdrop_position || 'tr'
  editBackdropBadgeDirection.value = s.backdrop_badge_direction || 'd'
  editBackdropEdgeInsetX.value = s.backdrop_edge_inset_x ?? 0
  editBackdropEdgeInsetY.value = s.backdrop_edge_inset_y ?? 0
  editEpisodeRatingsLimit.value = s.episode_ratings_limit ?? 1
  editEpisodeBadgeStyle.value = s.episode_badge_style || 'v'
  editEpisodeLabelStyle.value = s.episode_label_style || 'o'
  editEpisodeBadgeSize.value = s.episode_badge_size || 'l'
  editEpisodePosition.value = s.episode_position || 'tr'
  editEpisodeBadgeDirection.value = s.episode_badge_direction || 'v'
  editEpisodeBlur.value = s.episode_blur ?? false
  editSeasonRatingsLimit.value = s.season_ratings_limit ?? 3
  editSeasonBadgeStyle.value = s.season_badge_style || 'd'
  editSeasonLabelStyle.value = s.season_label_style || 'o'
  editSeasonBadgeSize.value = s.season_badge_size || 'm'
  editSeasonPosition.value = s.season_position || 'bc'
  editSeasonBadgeDirection.value = s.season_badge_direction || 'd'
  editPosterBadgeShape.value = s.poster_badge_shape || 'r'
  editLogoBadgeShape.value = s.logo_badge_shape || 'r'
  editBackdropBadgeShape.value = s.backdrop_badge_shape || 'r'
  editEpisodeBadgeShape.value = s.episode_badge_shape || 'r'
  editSeasonBadgeShape.value = s.season_badge_shape || 'r'
  editPosterBadgeBackground.value = s.poster_badge_background || 'd'
  editLogoBadgeBackground.value = s.logo_badge_background || 'd'
  editBackdropBadgeBackground.value = s.backdrop_badge_background || 'd'
  editEpisodeBadgeBackground.value = s.episode_badge_background || 'd'
  editSeasonBadgeBackground.value = s.season_badge_background || 'd'
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

// An emptied `v-model.number` input yields '' (and a partial entry can yield a
// non-integer); the backend types edge insets as a required i32, so send a clean,
// clamped integer to avoid a 400 mid-edit. Matches the server's 0–50 clamp.
function coerceInset(value: number): number {
  return Number.isFinite(value) ? Math.min(50, Math.max(0, Math.round(value))) : 0
}

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
      ratings_exclude: editRatingsExclude.value.join(','),
      poster_position: editPosterPosition.value,
      logo_ratings_limit: editLogoRatingsLimit.value,
      backdrop_ratings_limit: editBackdropRatingsLimit.value,
      poster_badge_style: editPosterBadgeStyle.value,
      logo_badge_style: editLogoBadgeStyle.value,
      backdrop_badge_style: editBackdropBadgeStyle.value,
      poster_label_style: editPosterLabelStyle.value,
      logo_label_style: editLogoLabelStyle.value,
      backdrop_label_style: editBackdropLabelStyle.value,
      poster_badge_direction: editPosterBadgeDirection.value,
      poster_badge_split: editPosterBadgeSplit.value,
      poster_fit: editPosterFit.value,
      poster_badge_size: editPosterBadgeSize.value,
      logo_badge_size: editLogoBadgeSize.value,
      backdrop_badge_size: editBackdropBadgeSize.value,
      backdrop_position: editBackdropPosition.value,
      backdrop_badge_direction: editBackdropBadgeDirection.value,
      backdrop_edge_inset_x: coerceInset(editBackdropEdgeInsetX.value),
      backdrop_edge_inset_y: coerceInset(editBackdropEdgeInsetY.value),
      episode_ratings_limit: editEpisodeRatingsLimit.value,
      episode_badge_style: editEpisodeBadgeStyle.value,
      episode_label_style: editEpisodeLabelStyle.value,
      episode_badge_size: editEpisodeBadgeSize.value,
      episode_position: editEpisodePosition.value,
      episode_badge_direction: editEpisodeBadgeDirection.value,
      episode_blur: editEpisodeBlur.value,
      season_ratings_limit: editSeasonRatingsLimit.value,
      season_badge_style: editSeasonBadgeStyle.value,
      season_label_style: editSeasonLabelStyle.value,
      season_badge_size: editSeasonBadgeSize.value,
      season_position: editSeasonPosition.value,
      season_badge_direction: editSeasonBadgeDirection.value,
      poster_badge_shape: editPosterBadgeShape.value,
      logo_badge_shape: editLogoBadgeShape.value,
      backdrop_badge_shape: editBackdropBadgeShape.value,
      episode_badge_shape: editEpisodeBadgeShape.value,
      season_badge_shape: editSeasonBadgeShape.value,
      poster_badge_background: editPosterBadgeBackground.value,
      logo_badge_background: editLogoBadgeBackground.value,
      backdrop_badge_background: editBackdropBadgeBackground.value,
      episode_badge_background: editEpisodeBadgeBackground.value,
      season_badge_background: editSeasonBadgeBackground.value,
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
  [editSource, editLang, editTextless, editRatingsLimit, editRatingsOrder, editRatingsExclude, editPosterPosition, editLogoRatingsLimit, editBackdropRatingsLimit, editPosterBadgeStyle, editLogoBadgeStyle, editBackdropBadgeStyle, editPosterLabelStyle, editLogoLabelStyle, editBackdropLabelStyle, editPosterBadgeDirection, editPosterBadgeSplit, editPosterFit, editPosterBadgeSize, editLogoBadgeSize, editBackdropBadgeSize, editBackdropPosition, editBackdropBadgeDirection, editBackdropEdgeInsetX, editBackdropEdgeInsetY, editEpisodeRatingsLimit, editEpisodeBadgeStyle, editEpisodeLabelStyle, editEpisodeBadgeSize, editEpisodePosition, editEpisodeBadgeDirection, editEpisodeBlur, editSeasonRatingsLimit, editSeasonBadgeStyle, editSeasonLabelStyle, editSeasonBadgeSize, editSeasonPosition, editSeasonBadgeDirection, editPosterBadgeShape, editLogoBadgeShape, editBackdropBadgeShape, editEpisodeBadgeShape, editSeasonBadgeShape, editPosterBadgeBackground, editLogoBadgeBackground, editBackdropBadgeBackground, editEpisodeBadgeBackground, editSeasonBadgeBackground],
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
const seasonPreview = ref<PreviewState>(makePreviewState())

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
  fetcher: (ratingsLimit: number, ratingsOrder: string) => Promise<Response>,
) {
  state.loading = true
  state.error = false
  const generation = ++state.generation

  try {
    const res = await fetcher(editRatingsLimit.value, editRatingsOrder.value.join(','))
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
let seasonPreviewTimer: ReturnType<typeof setTimeout> | null = null

function updatePosterPreview() {
  fetchPreviewImage(posterPreview.value, (_limit, order) => props.fetchPreview(editRatingsLimit.value, order, editPosterPosition.value, editPosterBadgeStyle.value, editPosterLabelStyle.value, editPosterBadgeDirection.value, editPosterBadgeSize.value, editRatingsExclude.value.join(','), editPosterBadgeSplit.value, editPosterBadgeShape.value, editPosterBadgeBackground.value, editPosterFit.value))
}

function updateLogoPreview() {
  if (props.fetchLogoPreview) {
    fetchPreviewImage(logoPreview.value, (_limit, order) => props.fetchLogoPreview!(editLogoRatingsLimit.value, order, editLogoBadgeStyle.value, editLogoLabelStyle.value, editLogoBadgeSize.value, editRatingsExclude.value.join(','), editLogoBadgeShape.value, editLogoBadgeBackground.value))
  }
}

function updateBackdropPreview() {
  if (props.fetchBackdropPreview) {
    fetchPreviewImage(backdropPreview.value, (_limit, order) => props.fetchBackdropPreview!(editBackdropRatingsLimit.value, order, editBackdropBadgeStyle.value, editBackdropLabelStyle.value, editBackdropBadgeSize.value, editBackdropPosition.value, editBackdropBadgeDirection.value, editRatingsExclude.value.join(','), editBackdropBadgeShape.value, editBackdropBadgeBackground.value, editBackdropEdgeInsetX.value, editBackdropEdgeInsetY.value))
  }
}

function updateEpisodePreview() {
  if (props.fetchEpisodePreview) {
    fetchPreviewImage(episodePreview.value, (_limit, order) => props.fetchEpisodePreview!(editEpisodeRatingsLimit.value, order, editEpisodeBadgeStyle.value, editEpisodeLabelStyle.value, editEpisodeBadgeSize.value, editEpisodePosition.value, editEpisodeBadgeDirection.value, editEpisodeBlur.value, editRatingsExclude.value.join(','), editEpisodeBadgeShape.value, editEpisodeBadgeBackground.value))
  }
}

function updateSeasonPreview() {
  if (props.fetchSeasonPreview) {
    fetchPreviewImage(seasonPreview.value, (_limit, order) => props.fetchSeasonPreview!(editSeasonRatingsLimit.value, order, editSeasonBadgeStyle.value, editSeasonLabelStyle.value, editSeasonBadgeSize.value, editSeasonPosition.value, editSeasonBadgeDirection.value, editRatingsExclude.value.join(','), editSeasonBadgeShape.value, editSeasonBadgeBackground.value))
  }
}

function updateAllPreviews() {
  updatePosterPreview()
  updateLogoPreview()
  updateBackdropPreview()
  updateEpisodePreview()
  updateSeasonPreview()
}

// Global settings: refresh all previews (order and exclude affect every type)
watch([editRatingsOrder, editRatingsExclude], () => {
  if (syncing) return
  if (posterPreviewTimer) clearTimeout(posterPreviewTimer)
  if (logoPreviewTimer) clearTimeout(logoPreviewTimer)
  if (backdropPreviewTimer) clearTimeout(backdropPreviewTimer)
  if (episodePreviewTimer) clearTimeout(episodePreviewTimer)
  if (seasonPreviewTimer) clearTimeout(seasonPreviewTimer)
  posterPreviewTimer = setTimeout(updatePosterPreview, 500)
  logoPreviewTimer = setTimeout(updateLogoPreview, 500)
  backdropPreviewTimer = setTimeout(updateBackdropPreview, 500)
  episodePreviewTimer = setTimeout(updateEpisodePreview, 500)
  seasonPreviewTimer = setTimeout(updateSeasonPreview, 500)
}, { deep: true })

// Poster-only settings
watch([editRatingsLimit, editPosterPosition, editPosterBadgeStyle, editPosterLabelStyle, editPosterBadgeDirection, editPosterBadgeSplit, editPosterFit, editPosterBadgeSize, editPosterBadgeShape, editPosterBadgeBackground], () => {
  if (syncing) return
  if (posterPreviewTimer) clearTimeout(posterPreviewTimer)
  posterPreviewTimer = setTimeout(updatePosterPreview, 500)
})

// Logo-only settings
watch([editLogoRatingsLimit, editLogoBadgeStyle, editLogoLabelStyle, editLogoBadgeSize, editLogoBadgeShape, editLogoBadgeBackground], () => {
  if (syncing) return
  if (logoPreviewTimer) clearTimeout(logoPreviewTimer)
  logoPreviewTimer = setTimeout(updateLogoPreview, 500)
})

// Backdrop-only settings
watch([editBackdropRatingsLimit, editBackdropBadgeStyle, editBackdropLabelStyle, editBackdropBadgeSize, editBackdropPosition, editBackdropBadgeDirection, editBackdropEdgeInsetX, editBackdropEdgeInsetY, editBackdropBadgeShape, editBackdropBadgeBackground], () => {
  if (syncing) return
  if (backdropPreviewTimer) clearTimeout(backdropPreviewTimer)
  backdropPreviewTimer = setTimeout(updateBackdropPreview, 500)
})

// Episode-only settings
watch([editEpisodeRatingsLimit, editEpisodeBadgeStyle, editEpisodeLabelStyle, editEpisodeBadgeSize, editEpisodePosition, editEpisodeBadgeDirection, editEpisodeBlur, editEpisodeBadgeShape, editEpisodeBadgeBackground], () => {
  if (syncing) return
  if (episodePreviewTimer) clearTimeout(episodePreviewTimer)
  episodePreviewTimer = setTimeout(updateEpisodePreview, 500)
})

// Season-only settings
watch([editSeasonRatingsLimit, editSeasonBadgeStyle, editSeasonLabelStyle, editSeasonBadgeSize, editSeasonPosition, editSeasonBadgeDirection, editSeasonBadgeShape, editSeasonBadgeBackground], () => {
  if (syncing) return
  if (seasonPreviewTimer) clearTimeout(seasonPreviewTimer)
  seasonPreviewTimer = setTimeout(updateSeasonPreview, 500)
})

// Initial preview on mount
updateAllPreviews()

onBeforeUnmount(() => {
  if (posterPreviewTimer) clearTimeout(posterPreviewTimer)
  if (logoPreviewTimer) clearTimeout(logoPreviewTimer)
  if (backdropPreviewTimer) clearTimeout(backdropPreviewTimer)
  if (episodePreviewTimer) clearTimeout(episodePreviewTimer)
  if (seasonPreviewTimer) clearTimeout(seasonPreviewTimer)
  if (posterPreview.value.src) URL.revokeObjectURL(posterPreview.value.src)
  if (logoPreview.value.src) URL.revokeObjectURL(logoPreview.value.src)
  if (backdropPreview.value.src) URL.revokeObjectURL(backdropPreview.value.src)
  if (episodePreview.value.src) URL.revokeObjectURL(episodePreview.value.src)
  if (seasonPreview.value.src) URL.revokeObjectURL(seasonPreview.value.src)
})

const inputId = (name: string) => props.uid ? `${name}-${props.uid}` : name

function isExcluded(key: string): boolean {
  return editRatingsExclude.value.includes(key)
}

function toggleExclude(key: string, checked: boolean) {
  const set = new Set(editRatingsExclude.value)
  if (checked) set.add(key)
  else set.delete(key)
  // Keep canonical source order so the saved value is stable regardless of click order.
  editRatingsExclude.value = ALL_RATING_SOURCES.map(s => s.key).filter(k => set.has(k))
}
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

      <div class="space-y-2 pt-1">
        <Label>Exclude ratings</Label>
        <p class="text-xs text-muted-foreground">Hide specific rating sources entirely. An excluded source never appears, freeing its slot for the next preferred source.</p>
        <div class="grid grid-cols-1 gap-x-4 gap-y-1.5 max-w-sm sm:grid-cols-2">
          <div
            v-for="source in ALL_RATING_SOURCES"
            :key="source.key"
            class="flex items-center gap-2"
          >
            <Checkbox
              :id="inputId(`exclude-${source.key}`)"
              :model-value="isExcluded(source.key)"
              :data-testid="`exclude-${source.key}-checkbox`"
              @update:model-value="(v) => toggleExclude(source.key, !!v)"
            />
            <span
              class="inline-block w-2.5 h-2.5 rounded-full shrink-0"
              :style="{ backgroundColor: source.color }"
            ></span>
            <Label :for="inputId(`exclude-${source.key}`)" class="text-sm font-normal">{{ source.label }}</Label>
          </div>
        </div>
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
              :disabled="editPosterBadgeShape === 'p'"
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
            <p v-if="editPosterBadgeShape === 'p'" class="text-xs text-muted-foreground">Pills always render horizontally.</p>
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
          <div class="space-y-2">
            <Label :for="inputId('poster-badge-shape')">Badge shape</Label>
            <Select
              :model-value="editPosterBadgeShape"
              @update:model-value="editPosterBadgeShape = $event as string"
            >
              <SelectTrigger :id="inputId('poster-badge-shape')" class="max-w-xs" data-testid="poster-badge-shape-select">
                <SelectValue placeholder="Select shape" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="r">Rounded</SelectItem>
                <SelectItem value="p">Pill</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-2">
            <Label :for="inputId('poster-badge-background')">Badge background</Label>
            <Select
              :model-value="editPosterBadgeBackground"
              @update:model-value="editPosterBadgeBackground = $event as string"
            >
              <SelectTrigger :id="inputId('poster-badge-background')" class="max-w-xs" data-testid="poster-badge-background-select">
                <SelectValue placeholder="Select background" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="d">Default</SelectItem>
                <SelectItem value="k">Dark</SelectItem>
                <SelectItem value="t">Transparent</SelectItem>
                <SelectItem value="n">None</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-2">
            <Label :for="inputId('poster-fit')">Aspect ratio</Label>
            <Select
              :model-value="editPosterFit"
              @update:model-value="editPosterFit = $event as string"
            >
              <SelectTrigger :id="inputId('poster-fit')" class="max-w-xs" data-testid="poster-fit-select">
                <SelectValue placeholder="Select fit" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="native">Native (source ratio)</SelectItem>
                <SelectItem value="cover">Crop to 2:3</SelectItem>
                <SelectItem value="blur">Blur fill to 2:3</SelectItem>
                <SelectItem value="pad">Letterbox to 2:3</SelectItem>
              </SelectContent>
            </Select>
            <p class="text-xs text-muted-foreground">
              How non-2:3 posters are fit to the standard 2:3 frame so clients don't crop them.
            </p>
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <Label :for="inputId('ratings-limit')">Max ratings</Label>
              <Input
                :id="inputId('ratings-limit')"
                v-model.number="editRatingsLimit"
                type="number"
                :min="0"
                :max="10"
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
      <div class="space-y-1">
        <div class="flex items-center gap-2">
          <Checkbox
            :id="inputId('poster-badge-split')"
            :model-value="editPosterBadgeSplit"
            data-testid="poster-badge-split-checkbox"
            @update:model-value="(v) => editPosterBadgeSplit = !!v"
          />
          <Label :for="inputId('poster-badge-split')">Split badges onto opposite sides</Label>
        </div>
        <p class="text-xs text-muted-foreground">
          Splits badges across two opposite sides — left/right for a vertical layout, top/bottom for horizontal rows.
        </p>
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
              :disabled="editLogoBadgeShape === 'p'"
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
            <p v-if="editLogoBadgeShape === 'p'" class="text-xs text-muted-foreground">Pills always render horizontally.</p>
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
          <div class="space-y-2">
            <Label :for="inputId('logo-badge-shape')">Badge shape</Label>
            <Select
              :model-value="editLogoBadgeShape"
              @update:model-value="editLogoBadgeShape = $event as string"
            >
              <SelectTrigger :id="inputId('logo-badge-shape')" class="max-w-xs" data-testid="logo-badge-shape-select">
                <SelectValue placeholder="Select shape" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="r">Rounded</SelectItem>
                <SelectItem value="p">Pill</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-2">
            <Label :for="inputId('logo-badge-background')">Badge background</Label>
            <Select
              :model-value="editLogoBadgeBackground"
              @update:model-value="editLogoBadgeBackground = $event as string"
            >
              <SelectTrigger :id="inputId('logo-badge-background')" class="max-w-xs" data-testid="logo-badge-background-select">
                <SelectValue placeholder="Select background" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="d">Default</SelectItem>
                <SelectItem value="k">Dark</SelectItem>
                <SelectItem value="t">Transparent</SelectItem>
                <SelectItem value="n">None</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <Label :for="inputId('logo-ratings-limit')">Max ratings</Label>
              <Input
                :id="inputId('logo-ratings-limit')"
                v-model.number="editLogoRatingsLimit"
                type="number"
                :min="0"
                :max="10"
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
              :disabled="editBackdropBadgeShape === 'p'"
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
            <p v-if="editBackdropBadgeShape === 'p'" class="text-xs text-muted-foreground">Pills always render horizontally.</p>
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
            <Label :for="inputId('backdrop-badge-shape')">Badge shape</Label>
            <Select
              :model-value="editBackdropBadgeShape"
              @update:model-value="editBackdropBadgeShape = $event as string"
            >
              <SelectTrigger :id="inputId('backdrop-badge-shape')" class="max-w-xs" data-testid="backdrop-badge-shape-select">
                <SelectValue placeholder="Select shape" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="r">Rounded</SelectItem>
                <SelectItem value="p">Pill</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-2">
            <Label :for="inputId('backdrop-badge-background')">Badge background</Label>
            <Select
              :model-value="editBackdropBadgeBackground"
              @update:model-value="editBackdropBadgeBackground = $event as string"
            >
              <SelectTrigger :id="inputId('backdrop-badge-background')" class="max-w-xs" data-testid="backdrop-badge-background-select">
                <SelectValue placeholder="Select background" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="d">Default</SelectItem>
                <SelectItem value="k">Dark</SelectItem>
                <SelectItem value="t">Transparent</SelectItem>
                <SelectItem value="n">None</SelectItem>
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
          <div v-if="backdropShowVerticalInset || backdropShowHorizontalInset" class="space-y-1">
            <Label>Distance from edge</Label>
            <div class="flex flex-wrap gap-4">
              <div v-if="backdropShowVerticalInset" class="space-y-1">
                <Label :for="inputId('backdrop-edge-inset-y')" class="text-xs font-normal text-muted-foreground">{{ backdropVerticalInsetLabel }}</Label>
                <div class="flex items-center gap-1">
                  <Input
                    :id="inputId('backdrop-edge-inset-y')"
                    v-model.number="editBackdropEdgeInsetY"
                    type="number"
                    :min="0"
                    :max="50"
                    class="w-[80px]"
                    data-testid="backdrop-edge-inset-y"
                  />
                  <span class="text-xs text-muted-foreground">%</span>
                </div>
              </div>
              <div v-if="backdropShowHorizontalInset" class="space-y-1">
                <Label :for="inputId('backdrop-edge-inset-x')" class="text-xs font-normal text-muted-foreground">{{ backdropHorizontalInsetLabel }}</Label>
                <div class="flex items-center gap-1">
                  <Input
                    :id="inputId('backdrop-edge-inset-x')"
                    v-model.number="editBackdropEdgeInsetX"
                    type="number"
                    :min="0"
                    :max="50"
                    class="w-[80px]"
                    data-testid="backdrop-edge-inset-x"
                  />
                  <span class="text-xs text-muted-foreground">%</span>
                </div>
              </div>
            </div>
            <p class="text-xs text-muted-foreground">Inset ratings from the image edge (% of size) — useful when a player crops the backdrop.</p>
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
              <Label :for="inputId('backdrop-ratings-limit')">Max ratings</Label>
              <Input
                :id="inputId('backdrop-ratings-limit')"
                v-model.number="editBackdropRatingsLimit"
                type="number"
                :min="0"
                :max="10"
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
              :disabled="editEpisodeBadgeShape === 'p'"
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
            <p v-if="editEpisodeBadgeShape === 'p'" class="text-xs text-muted-foreground">Pills always render horizontally.</p>
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
          <div class="space-y-2">
            <Label :for="inputId('episode-badge-shape')">Badge shape</Label>
            <Select
              :model-value="editEpisodeBadgeShape"
              @update:model-value="editEpisodeBadgeShape = $event as string"
            >
              <SelectTrigger :id="inputId('episode-badge-shape')" class="max-w-xs" data-testid="episode-badge-shape-select">
                <SelectValue placeholder="Select shape" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="r">Rounded</SelectItem>
                <SelectItem value="p">Pill</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-2">
            <Label :for="inputId('episode-badge-background')">Badge background</Label>
            <Select
              :model-value="editEpisodeBadgeBackground"
              @update:model-value="editEpisodeBadgeBackground = $event as string"
            >
              <SelectTrigger :id="inputId('episode-badge-background')" class="max-w-xs" data-testid="episode-badge-background-select">
                <SelectValue placeholder="Select background" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="d">Default</SelectItem>
                <SelectItem value="k">Dark</SelectItem>
                <SelectItem value="t">Transparent</SelectItem>
                <SelectItem value="n">None</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <Label :for="inputId('episode-ratings-limit')">Max ratings</Label>
              <Input
                :id="inputId('episode-ratings-limit')"
                v-model.number="editEpisodeRatingsLimit"
                type="number"
                :min="0"
                :max="10"
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

    <!-- Section 6: Season -->
    <div v-if="fetchSeasonPreview" class="rounded-md border p-4 space-y-3">
      <p class="text-sm font-semibold">Season</p>
      <div class="relative w-[170px]" :style="seasonPreview.size ? { aspectRatio: `${seasonPreview.size.w} / ${seasonPreview.size.h}` } : undefined">
        <img
          v-show="seasonPreview.src && !seasonPreview.error"
          :src="seasonPreview.src"
          alt="Season preview"
          class="rounded border w-full"
          @load="(e: Event) => onPreviewLoad(seasonPreview, e)"
          @error="seasonPreview.loading = false; seasonPreview.error = true"
        />
        <p v-if="seasonPreview.error && !seasonPreview.loading" class="text-sm text-muted-foreground py-4">Failed</p>
        <div v-if="seasonPreview.loading" class="absolute inset-0 flex items-center justify-center rounded">
          <Loader2 class="size-5 animate-spin text-white drop-shadow-md" />
        </div>
      </div>
      <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 max-w-lg">
          <div class="space-y-2">
            <Label :for="inputId('season-position')">Position</Label>
            <Select
              :model-value="editSeasonPosition"
              @update:model-value="editSeasonPosition = $event as string"
            >
              <SelectTrigger :id="inputId('season-position')" class="max-w-xs" data-testid="season-position-select">
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
            <Label :for="inputId('season-badge-direction')">Direction</Label>
            <Select
              :model-value="editSeasonBadgeDirection"
              @update:model-value="editSeasonBadgeDirection = $event as string"
            >
              <SelectTrigger :id="inputId('season-badge-direction')" class="max-w-xs" data-testid="season-badge-direction-select">
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
            <Label :for="inputId('season-badge-style')">Badge style</Label>
            <Select
              :model-value="editSeasonBadgeStyle"
              :disabled="editSeasonBadgeShape === 'p'"
              @update:model-value="editSeasonBadgeStyle = $event as string"
            >
              <SelectTrigger :id="inputId('season-badge-style')" class="max-w-xs" data-testid="season-badge-style-select">
                <SelectValue placeholder="Select style" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="d">Default</SelectItem>
                <SelectItem value="h">Horizontal</SelectItem>
                <SelectItem value="v">Vertical</SelectItem>
              </SelectContent>
            </Select>
            <p v-if="editSeasonBadgeShape === 'p'" class="text-xs text-muted-foreground">Pills always render horizontally.</p>
          </div>
          <div class="space-y-2">
            <Label :for="inputId('season-label-style')">Label style</Label>
            <Select
              :model-value="editSeasonLabelStyle"
              @update:model-value="editSeasonLabelStyle = $event as string"
            >
              <SelectTrigger :id="inputId('season-label-style')" class="max-w-xs" data-testid="season-label-style-select">
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
            <Label :for="inputId('season-badge-size')">Badge size</Label>
            <Select
              :model-value="editSeasonBadgeSize"
              @update:model-value="editSeasonBadgeSize = $event as string"
            >
              <SelectTrigger :id="inputId('season-badge-size')" class="max-w-xs" data-testid="season-badge-size-select">
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
            <Label :for="inputId('season-badge-shape')">Badge shape</Label>
            <Select
              :model-value="editSeasonBadgeShape"
              @update:model-value="editSeasonBadgeShape = $event as string"
            >
              <SelectTrigger :id="inputId('season-badge-shape')" class="max-w-xs" data-testid="season-badge-shape-select">
                <SelectValue placeholder="Select shape" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="r">Rounded</SelectItem>
                <SelectItem value="p">Pill</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-2">
            <Label :for="inputId('season-badge-background')">Badge background</Label>
            <Select
              :model-value="editSeasonBadgeBackground"
              @update:model-value="editSeasonBadgeBackground = $event as string"
            >
              <SelectTrigger :id="inputId('season-badge-background')" class="max-w-xs" data-testid="season-badge-background-select">
                <SelectValue placeholder="Select background" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="d">Default</SelectItem>
                <SelectItem value="k">Dark</SelectItem>
                <SelectItem value="t">Transparent</SelectItem>
                <SelectItem value="n">None</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <Label :for="inputId('season-ratings-limit')">Max ratings</Label>
              <Input
                :id="inputId('season-ratings-limit')"
                v-model.number="editSeasonRatingsLimit"
                type="number"
                :min="0"
                :max="10"
                class="w-[80px]"
                title="0 = no ratings"
              />
            </div>
            <p class="text-xs text-muted-foreground">0 = no ratings</p>
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
