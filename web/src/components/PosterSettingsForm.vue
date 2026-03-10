<script setup lang="ts">
import { ref, watch, onBeforeUnmount } from 'vue'
import { Save, Loader2, Check } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'

export interface PosterSettings {
  poster_source: string
  fanart_lang: string
  fanart_textless: boolean
  fanart_available: boolean
  ratings_limit: number
  ratings_order: string
  is_default?: boolean
}

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
  settings: PosterSettings
  uid?: string
  loadSettings: () => Promise<PosterSettings | null>
  saveSettings: (s: { poster_source: string; fanart_lang: string; fanart_textless: boolean; ratings_limit: number; ratings_order: string }) => Promise<string | null>
  resetSettings?: () => Promise<boolean>
  fetchPreview: (ratingsLimit: number, ratingsOrder: string) => Promise<Response>
}>()

const editSource = ref(props.settings.poster_source)
const editLang = ref(props.settings.fanart_lang)
const editTextless = ref(props.settings.fanart_textless)
const editRatingsLimit = ref(props.settings.ratings_limit)
const editRatingsOrder = ref<string[]>(parseOrder(props.settings.ratings_order))
const currentSettings = ref<PosterSettings>(props.settings)
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
  currentSettings.value = s
  editSource.value = s.poster_source
  editLang.value = s.fanart_lang
  editTextless.value = s.fanart_textless
  editRatingsLimit.value = s.ratings_limit
  editRatingsOrder.value = parseOrder(s.ratings_order)
})

async function handleSave() {
  if (saving.value) return
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
    })
    if (err) {
      error.value = err
    } else {
      const updated = await props.loadSettings()
      if (updated) {
        currentSettings.value = updated
        editSource.value = updated.poster_source
        editLang.value = updated.fanart_lang
        editTextless.value = updated.fanart_textless
        editRatingsLimit.value = updated.ratings_limit
        editRatingsOrder.value = parseOrder(updated.ratings_order)
      }
      showCheck.value = true
      checkTimeout = setTimeout(() => (showCheck.value = false), 1500)
    }
  } catch {
    error.value = 'Failed to save'
  } finally {
    saving.value = false
  }
}

async function handleReset() {
  if (!props.resetSettings) return
  saving.value = true
  error.value = ''
  showCheck.value = false
  if (checkTimeout) clearTimeout(checkTimeout)
  try {
    const ok = await props.resetSettings()
    if (ok) {
      const updated = await props.loadSettings()
      if (updated) {
        currentSettings.value = updated
        editSource.value = updated.poster_source
        editLang.value = updated.fanart_lang
        editTextless.value = updated.fanart_textless
        editRatingsLimit.value = updated.ratings_limit
        editRatingsOrder.value = parseOrder(updated.ratings_order)
      }
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

const previewSrc = ref('')
const previewLoading = ref(false)
const previewError = ref(false)
const previewSize = ref<{ w: number; h: number } | null>(null)
let previewTimer: ReturnType<typeof setTimeout> | null = null
let previewGeneration = 0

function onPreviewLoad(e: Event) {
  const img = e.target as HTMLImageElement
  if (img.naturalWidth && img.naturalHeight) {
    previewSize.value = { w: img.naturalWidth, h: img.naturalHeight }
  }
  previewLoading.value = false
  previewError.value = false
}

async function updatePreview() {
  previewLoading.value = true
  previewError.value = false
  const generation = ++previewGeneration

  try {
    const res = await props.fetchPreview(editRatingsLimit.value, editRatingsOrder.value.join(','))
    if (generation !== previewGeneration) return // stale response
    if (!res.ok) {
      previewError.value = true
      previewLoading.value = false
      return
    }
    const blob = await res.blob()
    if (generation !== previewGeneration) return
    if (previewSrc.value) URL.revokeObjectURL(previewSrc.value)
    previewSrc.value = URL.createObjectURL(blob)
  } catch {
    if (generation === previewGeneration) {
      previewError.value = true
      previewLoading.value = false
    }
  }
}

// Debounced watcher on rating settings
watch([editRatingsLimit, editRatingsOrder], () => {
  if (previewTimer) clearTimeout(previewTimer)
  previewTimer = setTimeout(updatePreview, 500)
}, { deep: true })

// Initial preview on mount
updatePreview()

onBeforeUnmount(() => {
  if (previewTimer) clearTimeout(previewTimer)
  if (previewSrc.value) URL.revokeObjectURL(previewSrc.value)
})

const inputId = (name: string) => props.uid ? `${name}-${props.uid}` : name
</script>

<template>
  <div class="space-y-4">
    <div class="flex items-center gap-2">
      <h3 class="text-sm font-semibold">Poster Settings</h3>
      <span
        v-if="resetSettings && currentSettings.is_default"
        class="text-xs bg-secondary text-secondary-foreground px-2 py-0.5 rounded"
      >
        Using defaults
      </span>
    </div>

    <div class="space-y-2">
      <label class="text-sm font-medium">Poster Source</label>
      <select
        v-model="editSource"
        class="flex h-9 w-full max-w-xs rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
      >
        <option value="tmdb">TMDB</option>
        <option value="fanart" :disabled="!currentSettings.fanart_available">
          Fanart.tv{{ !currentSettings.fanart_available ? ' (no API key)' : '' }}
        </option>
      </select>
    </div>

    <template v-if="editSource === 'fanart'">
      <div class="space-y-2">
        <label class="text-sm font-medium">Language</label>
        <Input
          v-model="editLang"
          type="text"
          placeholder="en"
          class="max-w-[120px]"
          maxlength="5"
          pattern="[a-zA-Z0-9\-]{2,5}"
          title="2-5 alphanumeric characters (e.g. en, pt-BR)"
        />
      </div>

      <div class="flex items-center gap-2">
        <input
          :id="inputId('textless')"
          v-model="editTextless"
          type="checkbox"
          class="h-4 w-4 rounded border-input"
        />
        <label :for="inputId('textless')" class="text-sm font-medium">Prefer textless posters</label>
      </div>
    </template>

    <div class="space-y-2 pt-2">
      <h3 class="text-sm font-semibold">Rating Display</h3>
      <div class="space-y-1 pb-2">
        <div class="flex items-center gap-3">
          <label :for="inputId('ratings-limit')" class="text-sm font-medium">Max ratings to show</label>
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

    <div class="space-y-2 pt-2">
      <h3 class="text-sm font-semibold">Preview</h3>
      <div class="relative max-w-[250px]" :style="previewSize ? { aspectRatio: `${previewSize.w} / ${previewSize.h}` } : undefined">
        <img
          v-show="previewSrc && !previewLoading && !previewError"
          :src="previewSrc"
          alt="Poster preview"
          class="rounded border w-full"
          @load="onPreviewLoad"
          @error="previewLoading = false; previewError = true"
        />
        <p v-if="previewError && !previewLoading" class="text-sm text-muted-foreground py-4">Failed to load preview</p>
        <div
          v-if="previewLoading"
          class="absolute inset-0 flex items-center justify-center bg-muted rounded border"
        >
          <Loader2 class="size-6 animate-spin text-muted-foreground" />
        </div>
      </div>
    </div>

    <div class="flex items-center gap-3 pt-1">
      <Button size="sm" :disabled="saving" @click="handleSave">
        <span class="relative size-4">
          <Transition
            enter-active-class="transition duration-200 ease-out"
            enter-from-class="opacity-0 scale-50"
            enter-to-class="opacity-100 scale-100"
            leave-active-class="transition duration-150 ease-in"
            leave-from-class="opacity-100 scale-100"
            leave-to-class="opacity-0 scale-50"
          >
            <Check v-if="showCheck" class="absolute inset-0 size-4 text-green-500" />
            <Loader2 v-else-if="saving" class="absolute inset-0 size-4 animate-spin" />
            <Save v-else class="absolute inset-0 size-4" />
          </Transition>
        </span>
        Save
      </Button>
      <Button
        v-if="resetSettings && !currentSettings.is_default"
        variant="outline"
        size="sm"
        :disabled="saving"
        @click="handleReset"
      >
        Reset to defaults
      </Button>
      <span v-if="error" class="text-sm text-destructive">{{ error }}</span>
    </div>
  </div>
</template>
