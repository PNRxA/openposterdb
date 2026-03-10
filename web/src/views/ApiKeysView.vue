<script setup lang="ts">
import { ref, reactive } from 'vue'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import { keysApi } from '@/lib/api'
import RefreshButton from '@/components/RefreshButton.vue'
import PosterSettingsForm from '@/components/PosterSettingsForm.vue'
import type { PosterSettings } from '@/components/PosterSettingsForm.vue'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Settings } from 'lucide-vue-next'

interface ApiKey {
  id: number
  name: string
  key_prefix: string
  created_at: string
  last_used_at: string | null
}

const queryClient = useQueryClient()

const { data: keys = ref([]), isFetching, refetch } = useQuery<ApiKey[]>({
  queryKey: ['api-keys'],
  queryFn: async () => {
    const res = await keysApi.list()
    if (!res.ok) throw new Error('Failed to fetch keys')
    return res.json()
  },
  initialData: [],
})

const newKeyName = ref('')
const newKeyValue = ref<string | null>(null)
const error = ref('')
const loading = ref(false)

// Per-key settings state
const expandedKey = ref<number | null>(null)
const keySettings = reactive<Record<number, PosterSettings>>({})
const settingsLoading = reactive<Record<number, boolean>>({})

async function toggleSettings(id: number) {
  if (expandedKey.value === id) {
    expandedKey.value = null
    return
  }
  expandedKey.value = id
  if (!keySettings[id]) {
    await fetchSettings(id)
  }
}

async function fetchSettings(id: number): Promise<PosterSettings | null> {
  settingsLoading[id] = true
  try {
    const res = await keysApi.getSettings(id)
    if (res.ok) {
      const data: PosterSettings = await res.json()
      keySettings[id] = data
      return data
    }
  } catch {
    // handled by caller
  } finally {
    settingsLoading[id] = false
  }
  return null
}

function makeLoadSettings(id: number) {
  return () => fetchSettings(id)
}

function makeSaveSettings(id: number) {
  return async (s: { poster_source: string; fanart_lang: string; fanart_textless: boolean }): Promise<string | null> => {
    const res = await keysApi.updateSettings(id, s)
    if (res.ok) return null
    const data = await res.json().catch(() => null)
    return data?.error || 'Failed to save'
  }
}

function makeResetSettings(id: number) {
  return async (): Promise<boolean> => {
    const res = await keysApi.deleteSettings(id)
    return res.ok
  }
}

async function createKey() {
  if (loading.value || !newKeyName.value.trim()) return
  error.value = ''
  loading.value = true
  try {
    const res = await keysApi.create(newKeyName.value.trim())
    if (res.ok) {
      const data = await res.json()
      newKeyValue.value = data.key
      newKeyName.value = ''
      queryClient.invalidateQueries({ queryKey: ['api-keys'] })
    } else {
      const data = await res.json()
      error.value = data.error || 'Failed to create key'
    }
  } catch {
    error.value = 'Failed to create key'
  } finally {
    loading.value = false
  }
}

async function deleteKey(id: number) {
  if (!confirm('Delete this API key? Any services using it will stop working.')) return
  error.value = ''
  try {
    const res = await keysApi.delete(id)
    if (res.ok) {
      queryClient.invalidateQueries({ queryKey: ['api-keys'] })
    } else {
      const data = await res.json().catch(() => null)
      error.value = data?.error || 'Failed to delete key'
    }
  } catch {
    error.value = 'Failed to delete key'
  }
}
</script>

<template>
  <div class="space-y-8">
    <div class="flex items-center justify-between">
      <h1 class="text-2xl font-bold">API Keys</h1>
      <RefreshButton :fetching="isFetching" @refresh="refetch()" />
    </div>

    <!-- Create new key -->
    <div class="space-y-3">
      <h2 class="text-lg font-semibold">Create new key</h2>
      <form class="flex gap-2" @submit.prevent="createKey">
        <Input
          v-model="newKeyName"
          type="text"
          placeholder="Key name (e.g. jellyfin-prod)"
          required
          class="flex-1"
        />
        <Button type="submit" :disabled="loading">Create</Button>
      </form>
      <p v-if="error" class="text-sm text-destructive">{{ error }}</p>

      <!-- Show newly created key -->
      <div v-if="newKeyValue" class="rounded-md border border-yellow-500 bg-yellow-50 dark:bg-yellow-950 p-4 space-y-2">
        <p class="text-sm font-medium">Copy your API key now. It won't be shown again.</p>
        <code class="block text-sm bg-background border rounded px-3 py-2 break-all select-all">{{ newKeyValue }}</code>
        <Button variant="outline" size="sm" @click="newKeyValue = null">Dismiss</Button>
      </div>
    </div>

    <!-- Key list -->
    <div class="space-y-3">
      <h2 class="text-lg font-semibold">Existing keys</h2>
      <p v-if="keys.length === 0" class="text-sm text-muted-foreground">No API keys yet.</p>
      <div v-for="key in keys" :key="key.id" class="rounded-md border">
        <div class="flex items-center justify-between p-3">
          <div class="space-y-1">
            <p class="font-medium text-sm">{{ key.name }}</p>
            <p class="text-xs text-muted-foreground">
              <span class="font-mono">{{ key.key_prefix }}...</span>
              &middot; Created {{ key.created_at }}
              <template v-if="key.last_used_at"> &middot; Last used {{ key.last_used_at }}</template>
            </p>
          </div>
          <div class="flex items-center gap-2">
            <Button variant="outline" size="sm" @click="toggleSettings(key.id)">
              <Settings class="h-4 w-4" />
            </Button>
            <Button variant="destructive" size="sm" @click="deleteKey(key.id)">Delete</Button>
          </div>
        </div>

        <!-- Inline settings panel -->
        <div v-if="expandedKey === key.id" class="border-t px-3 py-4 bg-muted/30">
          <div v-if="settingsLoading[key.id]" class="text-sm text-muted-foreground">Loading settings...</div>
          <PosterSettingsForm
            v-else-if="keySettings[key.id]"
            :settings="keySettings[key.id]!"
            :uid="String(key.id)"
            :load-settings="makeLoadSettings(key.id)"
            :save-settings="makeSaveSettings(key.id)"
            :reset-settings="makeResetSettings(key.id)"
          />
        </div>
      </div>
    </div>
  </div>
</template>
