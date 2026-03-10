<script setup lang="ts">
import { ref, watch } from 'vue'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import { adminApi } from '@/lib/api'
import RefreshButton from '@/components/RefreshButton.vue'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'

interface GlobalSettings {
  poster_source: string
  fanart_lang: string
  fanart_textless: boolean
  fanart_available: boolean
}

const queryClient = useQueryClient()

const {
  data: settings,
  isFetching,
  refetch,
} = useQuery<GlobalSettings>({
  queryKey: ['global-settings'],
  queryFn: async () => {
    const res = await adminApi.getSettings()
    if (!res.ok) throw new Error('Failed to fetch settings')
    return res.json()
  },
})

const posterSource = ref('tmdb')
const fanartLang = ref('en')
const fanartTextless = ref(false)
const saving = ref(false)
const error = ref('')
const success = ref(false)

watch(settings, (s) => {
  if (s) {
    posterSource.value = s.poster_source
    fanartLang.value = s.fanart_lang
    fanartTextless.value = s.fanart_textless
  }
}, { immediate: true })

async function save() {
  if (saving.value) return
  error.value = ''
  success.value = false
  saving.value = true
  try {
    const res = await adminApi.updateSettings({
      poster_source: posterSource.value,
      fanart_lang: fanartLang.value,
      fanart_textless: fanartTextless.value,
    })
    if (res.ok) {
      success.value = true
      queryClient.invalidateQueries({ queryKey: ['global-settings'] })
      setTimeout(() => (success.value = false), 2000)
    } else {
      const data = await res.json().catch(() => null)
      error.value = data?.error || 'Failed to save settings'
    }
  } catch {
    error.value = 'Failed to save settings'
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <div class="space-y-8">
    <div class="flex items-center justify-between">
      <h1 class="text-2xl font-bold">Settings</h1>
      <RefreshButton :fetching="isFetching" @refresh="refetch()" />
    </div>

    <div class="max-w-lg space-y-6">
      <div class="rounded-lg border p-6 space-y-4">
        <h2 class="text-lg font-semibold">Global Poster Defaults</h2>
        <p class="text-sm text-muted-foreground">
          These defaults apply to all API keys unless overridden per-key.
        </p>

        <div class="space-y-2">
          <label class="text-sm font-medium">Poster Source</label>
          <select
            v-model="posterSource"
            class="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
          >
            <option value="tmdb">TMDB</option>
            <option value="fanart" :disabled="settings && !settings.fanart_available">
              Fanart.tv{{ settings && !settings.fanart_available ? ' (no API key configured)' : '' }}
            </option>
          </select>
        </div>

        <template v-if="posterSource === 'fanart'">
          <div class="space-y-2">
            <label class="text-sm font-medium">Language</label>
            <Input v-model="fanartLang" type="text" placeholder="en" class="max-w-[120px]" />
            <p class="text-xs text-muted-foreground">ISO 639-1 language code (e.g. en, de, fr)</p>
          </div>

          <div class="flex items-center gap-2">
            <input
              id="textless"
              v-model="fanartTextless"
              type="checkbox"
              class="h-4 w-4 rounded border-input"
            />
            <label for="textless" class="text-sm font-medium">Prefer textless posters</label>
          </div>
        </template>

        <div class="flex items-center gap-3 pt-2">
          <Button :disabled="saving" @click="save">
            {{ saving ? 'Saving...' : 'Save' }}
          </Button>
          <span v-if="success" class="text-sm text-green-600">Saved</span>
          <span v-if="error" class="text-sm text-destructive">{{ error }}</span>
        </div>
      </div>
    </div>
  </div>
</template>
