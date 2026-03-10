<script setup lang="ts">
import { ref } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import { adminApi } from '@/lib/api'
import RefreshButton from '@/components/RefreshButton.vue'
import PosterSettingsForm from '@/components/PosterSettingsForm.vue'
import type { PosterSettings } from '@/components/PosterSettingsForm.vue'

const {
  data: settings,
  isFetching,
  refetch,
} = useQuery<PosterSettings>({
  queryKey: ['global-settings'],
  queryFn: async () => {
    const res = await adminApi.getSettings()
    if (!res.ok) throw new Error('Failed to fetch settings')
    return res.json()
  },
})

async function loadSettings(): Promise<PosterSettings | null> {
  const res = await adminApi.getSettings()
  if (!res.ok) return null
  return res.json()
}

async function saveSettings(s: {
  poster_source: string
  fanart_lang: string
  fanart_textless: boolean
  ratings_limit: number
  ratings_order: string
}): Promise<string | null> {
  const res = await adminApi.updateSettings(s)
  if (res.ok) return null
  const data = await res.json().catch(() => null)
  return data?.error || 'Failed to save settings'
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

        <PosterSettingsForm
          v-if="settings"
          :settings="settings"
          uid="global"
          :load-settings="loadSettings"
          :save-settings="saveSettings"
        />
      </div>
    </div>
  </div>
</template>
