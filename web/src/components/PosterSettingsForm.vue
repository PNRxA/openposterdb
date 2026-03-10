<script setup lang="ts">
import { ref, watch } from 'vue'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'

export interface PosterSettings {
  poster_source: string
  fanart_lang: string
  fanart_textless: boolean
  fanart_available: boolean
  is_default?: boolean
}

const props = defineProps<{
  settings: PosterSettings
  uid?: string
  loadSettings: () => Promise<PosterSettings | null>
  saveSettings: (s: { poster_source: string; fanart_lang: string; fanart_textless: boolean }) => Promise<string | null>
  resetSettings?: () => Promise<boolean>
}>()

const editSource = ref(props.settings.poster_source)
const editLang = ref(props.settings.fanart_lang)
const editTextless = ref(props.settings.fanart_textless)
const currentSettings = ref<PosterSettings>(props.settings)
const saving = ref(false)
const error = ref('')
const success = ref(false)

watch(() => props.settings, (s) => {
  currentSettings.value = s
  editSource.value = s.poster_source
  editLang.value = s.fanart_lang
  editTextless.value = s.fanart_textless
})

async function handleSave() {
  if (saving.value) return
  saving.value = true
  error.value = ''
  success.value = false
  try {
    const err = await props.saveSettings({
      poster_source: editSource.value,
      fanart_lang: editLang.value,
      fanart_textless: editTextless.value,
    })
    if (err) {
      error.value = err
    } else {
      success.value = true
      const updated = await props.loadSettings()
      if (updated) {
        currentSettings.value = updated
        editSource.value = updated.poster_source
        editLang.value = updated.fanart_lang
        editTextless.value = updated.fanart_textless
      }
      setTimeout(() => (success.value = false), 2000)
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
  try {
    const ok = await props.resetSettings()
    if (ok) {
      const updated = await props.loadSettings()
      if (updated) {
        currentSettings.value = updated
        editSource.value = updated.poster_source
        editLang.value = updated.fanart_lang
        editTextless.value = updated.fanart_textless
      }
    } else {
      error.value = 'Failed to reset'
    }
  } catch {
    error.value = 'Failed to reset'
  } finally {
    saving.value = false
  }
}

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

    <div class="flex items-center gap-3 pt-1">
      <Button size="sm" :disabled="saving" @click="handleSave">
        {{ saving ? 'Saving...' : 'Save' }}
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
      <span v-if="success" class="text-sm text-green-600">Saved</span>
      <span v-if="error" class="text-sm text-destructive">{{ error }}</span>
    </div>
  </div>
</template>
