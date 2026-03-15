<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import { selfApi, type SaveSettingsPayload } from '@/lib/api'
import RenderSettingsForm from '@/components/RenderSettingsForm.vue'
import type { RenderSettings } from '@/components/RenderSettingsForm.vue'
import { Button } from '@/components/ui/button'

const auth = useAuthStore()
const router = useRouter()

const keyName = ref('')
const keyPrefix = ref('')
const settings = ref<RenderSettings | null>(null)
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

async function loadSettings(): Promise<RenderSettings | null> {
  try {
    const res = await selfApi.getSettings()
    if (res.ok) {
      const data: RenderSettings = await res.json()
      settings.value = data
      return data
    }
  } catch {
    // handled by form
  }
  return null
}

async function saveSettings(s: SaveSettingsPayload): Promise<string | null> {
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
  <main class="min-h-screen flex items-center justify-center">
    <div class="w-full max-w-md space-y-6">
      <div class="flex items-center justify-between">
        <div>
          <h1 class="text-2xl font-bold">Image Settings</h1>
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
        <RenderSettingsForm
          :settings="settings"
          uid="self"
          :load-settings="loadSettings"
          :save-settings="saveSettings"
          :reset-settings="resetSettings"
          :fetch-preview="selfApi.previewPoster"
          :fetch-logo-preview="selfApi.previewLogo"
          :fetch-backdrop-preview="selfApi.previewBackdrop"
        />
      </div>
    </div>
  </main>
</template>
