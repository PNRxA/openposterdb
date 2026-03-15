<script setup lang="ts">
import { ref, computed, watch, nextTick, onBeforeUnmount } from 'vue'
import { Loader2, Check } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import type { SaveSettingsPayload } from '@/lib/api'

export interface RenderSettings {
  poster_source: string
  fanart_lang: string
  fanart_textless: boolean
  fanart_available: boolean
  ratings_limit: number
  ratings_order: string
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
}

const LANGUAGES = [
  { code: 'ar', name: 'Arabic' },
  { code: 'bg', name: 'Bulgarian' },
  { code: 'ca', name: 'Catalan' },
  { code: 'zh', name: 'Chinese' },
  { code: 'hr', name: 'Croatian' },
  { code: 'cs', name: 'Czech' },
  { code: 'da', name: 'Danish' },
  { code: 'nl', name: 'Dutch' },
  { code: 'en', name: 'English' },
  { code: 'fi', name: 'Finnish' },
  { code: 'fr', name: 'French' },
  { code: 'de', name: 'German' },
  { code: 'el', name: 'Greek' },
  { code: 'he', name: 'Hebrew' },
  { code: 'hi', name: 'Hindi' },
  { code: 'hu', name: 'Hungarian' },
  { code: 'id', name: 'Indonesian' },
  { code: 'it', name: 'Italian' },
  { code: 'ja', name: 'Japanese' },
  { code: 'ko', name: 'Korean' },
  { code: 'ms', name: 'Malay' },
  { code: 'no', name: 'Norwegian' },
  { code: 'fa', name: 'Persian' },
  { code: 'pl', name: 'Polish' },
  { code: 'pt', name: 'Portuguese' },
  { code: 'ro', name: 'Romanian' },
  { code: 'ru', name: 'Russian' },
  { code: 'sr', name: 'Serbian' },
  { code: 'sk', name: 'Slovak' },
  { code: 'sl', name: 'Slovenian' },
  { code: 'es', name: 'Spanish' },
  { code: 'sv', name: 'Swedish' },
  { code: 'th', name: 'Thai' },
  { code: 'tr', name: 'Turkish' },
  { code: 'uk', name: 'Ukrainian' },
  { code: 'vi', name: 'Vietnamese' },
] as const

const ALL_RATING_SOURCES = [
  { key: 'imdb', label: 'IMDb', color: '#b4910f' },
  { key: 'tmdb', label: 'TMDB', color: '#019b58' },
  { key: 'rt', label: 'Rotten Tomatoes (Critics)', color: '#b92308' },
  { key: 'rta', label: 'Rotten Tomatoes (Audience)', color: '#b92308' },
  { key: 'mc', label: 'Metacritic', color: '#4b9626' },
  { key: 'trakt', label: 'Trakt', color: '#af0f2d' },
  { key: 'lb', label: 'Letterboxd', color: '#009b58' },
  { key: 'mal', label: 'MyAnimeList', color: '#223c78' },
] as const

const props = defineProps<{
  settings: RenderSettings
  uid?: string
  loadSettings: () => Promise<RenderSettings | null>
  saveSettings: (s: SaveSettingsPayload) => Promise<string | null>
  resetSettings?: () => Promise<boolean>
  fetchPreview: (ratingsLimit: number, ratingsOrder: string, posterPosition?: string, badgeStyle?: string, labelStyle?: string, badgeDirection?: string) => Promise<Response>
  fetchLogoPreview?: (ratingsLimit: number, ratingsOrder: string, badgeStyle?: string, labelStyle?: string) => Promise<Response>
  fetchBackdropPreview?: (ratingsLimit: number, ratingsOrder: string, badgeStyle?: string, labelStyle?: string) => Promise<Response>
}>()

const editFanart = ref(props.settings.poster_source === 'f')
const editLang = ref(props.settings.fanart_lang || 'en')
const editTextless = ref(props.settings.fanart_textless)
const editSource = computed(() => editFanart.value ? 'f' : 't')
const editRatingsLimit = ref(props.settings.ratings_limit)
const editRatingsOrder = ref<string[]>(parseOrder(props.settings.ratings_order))
const editPosterPosition = ref(props.settings.poster_position || 'bc')
const editLogoRatingsLimit = ref(props.settings.logo_ratings_limit ?? 3)
const editBackdropRatingsLimit = ref(props.settings.backdrop_ratings_limit ?? 3)
const editPosterBadgeStyle = ref(props.settings.poster_badge_style || 'd')
const editLogoBadgeStyle = ref(props.settings.logo_badge_style || 'v')
const editBackdropBadgeStyle = ref(props.settings.backdrop_badge_style || 'v')
const editPosterLabelStyle = ref(props.settings.poster_label_style || 'i')
const editLogoLabelStyle = ref(props.settings.logo_label_style || 'i')
const editBackdropLabelStyle = ref(props.settings.backdrop_label_style || 'i')
const editPosterBadgeDirection = ref(props.settings.poster_badge_direction || 'd')

function applySettings(s: RenderSettings) {
  editFanart.value = s.poster_source === 'f'
  editLang.value = s.fanart_lang || 'en'
  editTextless.value = s.fanart_textless
  editRatingsLimit.value = s.ratings_limit
  editRatingsOrder.value = parseOrder(s.ratings_order)
  editPosterPosition.value = s.poster_position || 'bc'
  editLogoRatingsLimit.value = s.logo_ratings_limit ?? 3
  editBackdropRatingsLimit.value = s.backdrop_ratings_limit ?? 3
  editPosterBadgeStyle.value = s.poster_badge_style || 'd'
  editLogoBadgeStyle.value = s.logo_badge_style || 'v'
  editBackdropBadgeStyle.value = s.backdrop_badge_style || 'v'
  editPosterLabelStyle.value = s.poster_label_style || 'i'
  editLogoLabelStyle.value = s.logo_label_style || 'i'
  editBackdropLabelStyle.value = s.backdrop_label_style || 'i'
  editPosterBadgeDirection.value = s.poster_badge_direction || 'd'
}
const currentSettings = ref<RenderSettings>(props.settings)
const saving = ref(false)
const error = ref('')
const showCheck = ref(false)
let checkTimeout: ReturnType<typeof setTimeout> | null = null
function parseOrder(order: string): string[] {
  const keys = order ? order.split(',').map(k => k.trim()).filter(Boolean) : []
  // Ensure all sources are present — add any missing ones at the end
  const allKeys = ALL_RATING_SOURCES.map(s => s.key)
  for (const k of allKeys) {
    if (!keys.includes(k)) keys.push(k)
  }
  return keys
}

function moveItem(from: number, to: number) {
  if (to < 0 || to >= editRatingsOrder.value.length) return
  const arr = [...editRatingsOrder.value]
  const removed = arr.splice(from, 1)
  arr.splice(to, 0, ...removed)
  editRatingsOrder.value = arr
}

function getRatingSource(key: string) {
  return ALL_RATING_SOURCES.find(s => s.key === key)
}

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
      poster_source: editSource.value,
      fanart_lang: editLang.value,
      fanart_textless: editTextless.value,
      ratings_limit: editRatingsLimit.value,
      ratings_order: editRatingsOrder.value.join(','),
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
  [editSource, editLang, editTextless, editRatingsLimit, editRatingsOrder, editPosterPosition, editLogoRatingsLimit, editBackdropRatingsLimit, editPosterBadgeStyle, editLogoBadgeStyle, editBackdropBadgeStyle, editPosterLabelStyle, editLogoLabelStyle, editBackdropLabelStyle, editPosterBadgeDirection],
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
  fetcher: (ratingsLimit: number, ratingsOrder: string, posterPosition?: string, badgeStyle?: string, labelStyle?: string, badgeDirection?: string) => Promise<Response>,
  extraArgs?: { posterPosition?: string; badgeStyle?: string; labelStyle?: string; badgeDirection?: string },
) {
  state.loading = true
  state.error = false
  const generation = ++state.generation

  try {
    const res = await fetcher(editRatingsLimit.value, editRatingsOrder.value.join(','), extraArgs?.posterPosition, extraArgs?.badgeStyle, extraArgs?.labelStyle, extraArgs?.badgeDirection)
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

function updatePosterPreview() {
  fetchPreviewImage(posterPreview.value, props.fetchPreview, { posterPosition: editPosterPosition.value, badgeStyle: editPosterBadgeStyle.value, labelStyle: editPosterLabelStyle.value, badgeDirection: editPosterBadgeDirection.value })
}

function updateLogoPreview() {
  if (props.fetchLogoPreview) {
    fetchPreviewImage(logoPreview.value, (_limit, order) => props.fetchLogoPreview!(editLogoRatingsLimit.value, order, editLogoBadgeStyle.value, editLogoLabelStyle.value))
  }
}

function updateBackdropPreview() {
  if (props.fetchBackdropPreview) {
    fetchPreviewImage(backdropPreview.value, (_limit, order) => props.fetchBackdropPreview!(editBackdropRatingsLimit.value, order, editBackdropBadgeStyle.value, editBackdropLabelStyle.value))
  }
}

function updateAllPreviews() {
  updatePosterPreview()
  updateLogoPreview()
  updateBackdropPreview()
}

// Global settings: refresh all previews
watch([editRatingsOrder], () => {
  if (syncing) return
  if (posterPreviewTimer) clearTimeout(posterPreviewTimer)
  if (logoPreviewTimer) clearTimeout(logoPreviewTimer)
  if (backdropPreviewTimer) clearTimeout(backdropPreviewTimer)
  posterPreviewTimer = setTimeout(updatePosterPreview, 500)
  logoPreviewTimer = setTimeout(updateLogoPreview, 500)
  backdropPreviewTimer = setTimeout(updateBackdropPreview, 500)
}, { deep: true })

// Poster-only settings
watch([editRatingsLimit, editPosterPosition, editPosterBadgeStyle, editPosterLabelStyle, editPosterBadgeDirection], () => {
  if (syncing) return
  if (posterPreviewTimer) clearTimeout(posterPreviewTimer)
  posterPreviewTimer = setTimeout(updatePosterPreview, 500)
})

// Logo-only settings
watch([editLogoRatingsLimit, editLogoBadgeStyle, editLogoLabelStyle], () => {
  if (syncing) return
  if (logoPreviewTimer) clearTimeout(logoPreviewTimer)
  logoPreviewTimer = setTimeout(updateLogoPreview, 500)
})

// Backdrop-only settings
watch([editBackdropRatingsLimit, editBackdropBadgeStyle, editBackdropLabelStyle], () => {
  if (syncing) return
  if (backdropPreviewTimer) clearTimeout(backdropPreviewTimer)
  backdropPreviewTimer = setTimeout(updateBackdropPreview, 500)
})

// Initial preview on mount
updateAllPreviews()

onBeforeUnmount(() => {
  if (posterPreviewTimer) clearTimeout(posterPreviewTimer)
  if (logoPreviewTimer) clearTimeout(logoPreviewTimer)
  if (backdropPreviewTimer) clearTimeout(backdropPreviewTimer)
  if (posterPreview.value.src) URL.revokeObjectURL(posterPreview.value.src)
  if (logoPreview.value.src) URL.revokeObjectURL(logoPreview.value.src)
  if (backdropPreview.value.src) URL.revokeObjectURL(backdropPreview.value.src)
})

let syncing = false
const inputId = (name: string) => props.uid ? `${name}-${props.uid}` : name
const selectBaseClass = 'flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring'
const selectClass = selectBaseClass + ' max-w-xs'
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

    <!-- Fanart options -->
    <template v-if="currentSettings.fanart_available">
      <div class="flex items-center gap-2">
        <input
          :id="inputId('fanart')"
          v-model="editFanart"
          type="checkbox"
          class="h-4 w-4 rounded border-input"
          data-testid="fanart-checkbox"
        />
        <label :for="inputId('fanart')" class="text-sm font-medium">Use Fanart.tv for custom language and textless posters</label>
      </div>

      <div class="pl-6 space-y-3" :class="{ 'opacity-50': !editFanart }">
        <div class="space-y-1">
          <div class="flex items-center gap-3">
            <label :for="inputId('fanart-lang')" class="text-sm font-medium">Language</label>
            <select
              :id="inputId('fanart-lang')"
              v-model="editLang"
              :class="selectBaseClass"
              class="max-w-[200px]"
              :disabled="!editFanart"
              data-testid="fanart-lang-select"
            >
              <option v-for="lang in LANGUAGES" :key="lang.code" :value="lang.code">
                {{ lang.code }} - {{ lang.name }}
              </option>
            </select>
          </div>
          <p class="text-xs text-muted-foreground">Best effort — falls back to English if unavailable.</p>
        </div>

        <div class="flex items-center gap-2">
          <input
            :id="inputId('textless')"
            v-model="editTextless"
            type="checkbox"
            class="h-4 w-4 rounded border-input"
            :disabled="!editFanart"
            data-testid="textless-checkbox"
          />
          <label :for="inputId('textless')" class="text-sm font-medium">Prefer textless posters</label>
        </div>
      </div>
    </template>
    <p v-else class="text-sm text-muted-foreground">
      Fanart.tv options require a <span class="font-medium">Fanart.tv API key</span>.
    </p>

    <div class="space-y-2 pt-2">
      <p class="text-sm font-semibold">Rating Display</p>

      <div class="space-y-2">
        <label class="text-sm font-medium">Rating order</label>
        <p class="text-xs text-muted-foreground">Use the arrows to reorder. Higher items have priority.</p>
        <div class="space-y-1 max-w-sm">
          <div
            v-for="(key, index) in editRatingsOrder"
            :key="key"
            class="flex items-center gap-2 rounded border px-2 py-1.5 bg-background"
          >
            <span class="text-muted-foreground text-xs select-none w-4 text-right">{{ index + 1 }}</span>
            <span
              class="inline-block w-2.5 h-2.5 rounded-full shrink-0"
              :style="{ backgroundColor: getRatingSource(key)?.color }"
            ></span>
            <span class="text-sm flex-1">{{ getRatingSource(key)?.label || key }}</span>
            <button
              type="button"
              class="inline-flex items-center justify-center w-8 h-8 rounded border text-muted-foreground hover:text-foreground hover:bg-muted disabled:opacity-30 disabled:pointer-events-none"
              :disabled="index === 0"
              @click="moveItem(index, index - 1)"
              title="Move up"
            >&uarr;</button>
            <button
              type="button"
              class="inline-flex items-center justify-center w-8 h-8 rounded border text-muted-foreground hover:text-foreground hover:bg-muted disabled:opacity-30 disabled:pointer-events-none"
              :disabled="index === editRatingsOrder.length - 1"
              @click="moveItem(index, index + 1)"
              title="Move down"
            >&darr;</button>
          </div>
        </div>
      </div>
    </div>

    <!-- Section 2: Poster -->
    <div class="rounded-md border p-4 space-y-3">
      <div class="flex gap-4 items-start">
        <div class="space-y-1 shrink-0">
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
        </div>
        <div class="space-y-3 flex-1 min-w-0">
          <div class="space-y-2">
            <label :for="inputId('poster-position')" class="text-sm font-medium">Badge position</label>
            <select
              :id="inputId('poster-position')"
              v-model="editPosterPosition"
              data-testid="poster-position-select"
              :class="selectClass"
            >
              <option value="bc">Bottom Center</option>
              <option value="tc">Top Center</option>
              <option value="l">Left</option>
              <option value="r">Right</option>
              <option value="tl">Top Left</option>
              <option value="tr">Top Right</option>
              <option value="bl">Bottom Left</option>
              <option value="br">Bottom Right</option>
            </select>
          </div>
          <div class="space-y-2">
            <label :for="inputId('poster-badge-direction')" class="text-sm font-medium">Badge direction</label>
            <select
              :id="inputId('poster-badge-direction')"
              v-model="editPosterBadgeDirection"
              data-testid="poster-badge-direction-select"
              :class="selectClass"
            >
              <option value="d">Default</option>
              <option value="h">Horizontal</option>
              <option value="v">Vertical</option>
            </select>
          </div>
          <div class="space-y-2">
            <label :for="inputId('poster-badge-style')" class="text-sm font-medium">Badge style</label>
            <select
              :id="inputId('poster-badge-style')"
              v-model="editPosterBadgeStyle"
              :class="selectClass"
            >
              <option value="d">Default</option>
              <option value="h">Horizontal</option>
              <option value="v">Vertical</option>
            </select>
          </div>
          <div class="space-y-2">
            <label :for="inputId('poster-label-style')" class="text-sm font-medium">Label style</label>
            <select
              :id="inputId('poster-label-style')"
              v-model="editPosterLabelStyle"
              :class="selectClass"
            >
              <option value="t">Text</option>
              <option value="i">Icon</option>
            </select>
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <label :for="inputId('ratings-limit')" class="text-sm font-medium">Max ratings</label>
              <Input
                :id="inputId('ratings-limit')"
                v-model.number="editRatingsLimit"
                type="number"
                :min="0"
                :max="8"
                class="w-[80px]"
                title="0 = show all"
              />
            </div>
            <p class="text-xs text-muted-foreground">0 = show all available ratings</p>
          </div>
        </div>
      </div>
    </div>

    <!-- Section 3: Logo -->
    <div v-if="fetchLogoPreview" class="rounded-md border p-4 space-y-3">
      <div class="flex gap-4 items-start">
        <div class="space-y-1 shrink-0">
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
        </div>
        <div class="space-y-3 flex-1 min-w-0">
          <div class="space-y-2">
            <label :for="inputId('logo-badge-style')" class="text-sm font-medium">Badge style</label>
            <select
              :id="inputId('logo-badge-style')"
              v-model="editLogoBadgeStyle"
              :class="selectClass"
            >
              <option value="h">Horizontal</option>
              <option value="v">Vertical</option>
            </select>
          </div>
          <div class="space-y-2">
            <label :for="inputId('logo-label-style')" class="text-sm font-medium">Label style</label>
            <select
              :id="inputId('logo-label-style')"
              v-model="editLogoLabelStyle"
              :class="selectClass"
            >
              <option value="t">Text</option>
              <option value="i">Icon</option>
            </select>
          </div>
          <div class="flex items-center gap-3">
            <label :for="inputId('logo-ratings-limit')" class="text-sm font-medium">Max ratings</label>
            <Input
              :id="inputId('logo-ratings-limit')"
              v-model.number="editLogoRatingsLimit"
              type="number"
              :min="0"
              :max="8"
              class="w-[80px]"
              title="0 = show all"
            />
          </div>
        </div>
      </div>
    </div>

    <!-- Section 4: Backdrop -->
    <div v-if="fetchBackdropPreview" class="rounded-md border p-4 space-y-3">
      <div class="flex gap-4 items-start">
        <div class="space-y-1 shrink-0">
          <p class="text-sm font-semibold">Backdrop</p>
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
        </div>
        <div class="space-y-3 flex-1 min-w-0">
          <div class="space-y-2">
            <label :for="inputId('backdrop-badge-style')" class="text-sm font-medium">Badge style</label>
            <select
              :id="inputId('backdrop-badge-style')"
              v-model="editBackdropBadgeStyle"
              :class="selectClass"
            >
              <option value="h">Horizontal</option>
              <option value="v">Vertical</option>
            </select>
          </div>
          <div class="space-y-2">
            <label :for="inputId('backdrop-label-style')" class="text-sm font-medium">Label style</label>
            <select
              :id="inputId('backdrop-label-style')"
              v-model="editBackdropLabelStyle"
              :class="selectClass"
            >
              <option value="t">Text</option>
              <option value="i">Icon</option>
            </select>
          </div>
          <div class="flex items-center gap-3">
            <label :for="inputId('backdrop-ratings-limit')" class="text-sm font-medium">Max ratings</label>
            <Input
              :id="inputId('backdrop-ratings-limit')"
              v-model.number="editBackdropRatingsLimit"
              type="number"
              :min="0"
              :max="8"
              class="w-[80px]"
              title="0 = show all"
            />
          </div>
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
