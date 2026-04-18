<script setup lang="ts">
import { ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import { adminApi, type SaveSettingsPayload } from '@/lib/api'
import { FREE_API_KEY } from '@/lib/constants'
import RefreshButton from '@/components/RefreshButton.vue'
import RenderSettingsForm from '@/components/RenderSettingsForm.vue'
import type { RenderSettings } from '@/components/RenderSettingsForm.vue'

type SettingsResponse = RenderSettings & { free_api_key_enabled: boolean; free_api_key_locked: boolean }

const freeApiKeyEnabled = ref(false)
const freeKeyLoading = ref(false)
const freeKeyError = ref('')

const {
  data: settings,
  isFetching,
  refetch,
} = useQuery<SettingsResponse>({
  queryKey: ['global-settings'],
  queryFn: async () => {
    const res = await adminApi.getSettings()
    if (!res.ok) throw new Error('Failed to fetch settings')
    return res.json()
  },
})

watch(settings, (s) => {
  if (s) freeApiKeyEnabled.value = s.free_api_key_enabled
}, { immediate: true })

async function loadSettings(): Promise<RenderSettings | null> {
  const res = await adminApi.getSettings()
  if (!res.ok) return null
  return res.json()
}

async function saveSettings(s: SaveSettingsPayload): Promise<string | null> {
  const res = await adminApi.updateSettings(s)
  if (res.ok) return null
  const data = await res.json().catch(() => null)
  return data?.error || 'Failed to save settings'
}

async function toggleFreeApiKey() {
  if (!settings.value) return
  freeKeyLoading.value = true
  freeKeyError.value = ''
  const newVal = !freeApiKeyEnabled.value
  const s = settings.value
  const res = await adminApi.updateSettings({
    image_source: s.image_source,
    lang: s.lang,
    textless: s.textless,
    ratings_limit: s.ratings_limit,
    ratings_order: s.ratings_order,
    poster_position: s.poster_position,
    logo_ratings_limit: s.logo_ratings_limit,
    backdrop_ratings_limit: s.backdrop_ratings_limit,
    poster_badge_style: s.poster_badge_style,
    poster_badge_background_style: s.poster_badge_background_style,
    logo_badge_style: s.logo_badge_style,
    backdrop_badge_style: s.backdrop_badge_style,
    poster_label_style: s.poster_label_style,
    logo_label_style: s.logo_label_style,
    backdrop_label_style: s.backdrop_label_style,
    poster_badge_direction: s.poster_badge_direction,
    poster_badge_size: s.poster_badge_size,
    poster_badge_scale_override: s.poster_badge_scale_override,
    logo_badge_size: s.logo_badge_size,
    logo_badge_scale_override: s.logo_badge_scale_override,
    backdrop_badge_size: s.backdrop_badge_size,
    backdrop_badge_scale_override: s.backdrop_badge_scale_override,
    episode_ratings_limit: s.episode_ratings_limit,
    episode_badge_style: s.episode_badge_style,
    episode_label_style: s.episode_label_style,
    episode_badge_size: s.episode_badge_size,
    episode_badge_scale_override: s.episode_badge_scale_override,
    episode_position: s.episode_position,
    episode_badge_direction: s.episode_badge_direction,
    episode_blur: s.episode_blur,
    poster_badge_gap: s.poster_badge_gap,
    poster_badge_margin: s.poster_badge_margin,
    logo_badge_gap: s.logo_badge_gap,
    logo_badge_margin: s.logo_badge_margin,
    backdrop_badge_gap: s.backdrop_badge_gap,
    backdrop_badge_margin: s.backdrop_badge_margin,
    episode_badge_gap: s.episode_badge_gap,
    episode_badge_margin: s.episode_badge_margin,
    free_api_key_enabled: newVal,
  })
  if (res.ok) {
    freeApiKeyEnabled.value = newVal
  } else {
    const data = await res.json().catch(() => null)
    freeKeyError.value = data?.error || 'Failed to save'
  }
  freeKeyLoading.value = false
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
        <h2 class="text-lg font-semibold">Free API Key</h2>
        <p class="text-sm text-muted-foreground">
          When enabled, the key <code class="font-mono text-xs bg-muted px-1 py-0.5 rounded">{{ FREE_API_KEY }}</code>
          can be used for poster serving with global default settings.
          It does not grant access to self-service features.
        </p>
        <label class="flex items-center gap-3 cursor-pointer">
          <button
            type="button"
            role="switch"
            :aria-checked="freeApiKeyEnabled"
            :disabled="freeKeyLoading || !settings || settings?.free_api_key_locked"
            class="relative inline-flex h-5 w-9 shrink-0 rounded-full border-2 border-transparent transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
            :class="freeApiKeyEnabled ? 'bg-primary' : 'bg-input'"
            @click="toggleFreeApiKey"
          >
            <span
              class="pointer-events-none block h-4 w-4 rounded-full bg-background shadow-lg ring-0 transition-transform"
              :class="freeApiKeyEnabled ? 'translate-x-4' : 'translate-x-0'"
            />
          </button>
          <span class="text-sm font-medium">{{ freeApiKeyEnabled ? 'Enabled' : 'Disabled' }}</span>
          <span v-if="freeKeyError" class="text-sm text-destructive">{{ freeKeyError }}</span>
        </label>
        <p v-if="settings?.free_api_key_locked" class="text-sm text-muted-foreground">
          Controlled by <code class="font-mono text-xs bg-muted px-1 py-0.5 rounded">FREE_KEY_ENABLED</code> environment variable.
        </p>
      </div>

      <div class="rounded-lg border p-6 space-y-4">
        <h2 class="text-lg font-semibold">Global Image Settings</h2>
        <p class="text-sm text-muted-foreground">
          These defaults apply to all API keys unless overridden per-key.
        </p>

        <RenderSettingsForm
          v-if="settings"
          :settings="settings"
          uid="global"
          :load-settings="loadSettings"
          :save-settings="saveSettings"
          :fetch-preview="adminApi.previewPoster"
          :fetch-logo-preview="adminApi.previewLogo"
          :fetch-backdrop-preview="adminApi.previewBackdrop"
          :fetch-episode-preview="adminApi.previewEpisode"
        />
      </div>
    </div>
  </div>
</template>
