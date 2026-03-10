<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import { selfApi } from '@/lib/api'
import PosterSettingsForm from '@/components/PosterSettingsForm.vue'
import type { PosterSettings } from '@/components/PosterSettingsForm.vue'
import { Button } from '@/components/ui/button'

const auth = useAuthStore()
const router = useRouter()

const keyName = ref('')
const keyPrefix = ref('')
const settings = ref<PosterSettings | null>(null)
const settingsLoading = ref(true)
const initError = ref('')

onMounted(async () => {
  try {
    const [infoRes, settingsRes] = await Promise.all([
      selfApi.getInfo(),
      selfApi.getSettings(),
    ])

    if (!infoRes.ok || !settingsRes.ok) {
      auth.logoutApiKey()
      router.replace('/login')
      return
    }

    const info = await infoRes.json()
    keyName.value = info.name
    keyPrefix.value = info.key_prefix
    auth.apiKeyInfo = { name: info.name, key_prefix: info.key_prefix }

    settings.value = await settingsRes.json()
  } catch {
    initError.value = 'Failed to load settings'
  } finally {
    settingsLoading.value = false
  }
})

async function loadSettings(): Promise<PosterSettings | null> {
  try {
    const res = await selfApi.getSettings()
    if (res.ok) {
      const data: PosterSettings = await res.json()
      settings.value = data
      return data
    }
  } catch {
    // handled by form
  }
  return null
}

async function saveSettings(s: { poster_source: string; fanart_lang: string; fanart_textless: boolean; ratings_limit: number; ratings_order: string }): Promise<string | null> {
  const res = await selfApi.updateSettings(s)
  if (res.ok) return null
  const data = await res.json().catch(() => null)
  return data?.error || 'Failed to save'
}

async function resetSettings(): Promise<boolean> {
  const res = await selfApi.resetSettings()
  return res.ok
}

function handleLogout() {
  auth.logoutApiKey()
  router.push('/login')
}
</script>

<template>
  <div class="min-h-screen flex items-center justify-center">
    <div class="w-full max-w-md space-y-6">
      <div class="flex items-center justify-between">
        <div>
          <h1 class="text-2xl font-bold">Poster Settings</h1>
          <p v-if="keyName" class="text-sm text-muted-foreground">
            {{ keyName }}
            <span class="font-mono">({{ keyPrefix }}...)</span>
          </p>
        </div>
        <Button variant="outline" size="sm" @click="handleLogout">Logout</Button>
      </div>

      <div v-if="settingsLoading" class="text-sm text-muted-foreground">Loading settings...</div>
      <div v-else-if="initError" class="text-sm text-destructive">{{ initError }}</div>

      <div v-else-if="settings" class="rounded-md border p-4">
        <PosterSettingsForm
          :settings="settings"
          uid="self"
          :load-settings="loadSettings"
          :save-settings="saveSettings"
          :reset-settings="resetSettings"
          :fetch-preview="selfApi.previewPoster"
        />
      </div>
    </div>
  </div>
</template>
