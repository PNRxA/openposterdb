<script setup lang="ts">
import { ref, reactive } from 'vue'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import { keysApi } from '@/lib/api'
import RefreshButton from '@/components/RefreshButton.vue'
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

interface KeySettings {
  poster_source: string
  fanart_lang: string
  fanart_textless: boolean
  fanart_available: boolean
  is_default: boolean
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
const keySettings = reactive<Record<number, KeySettings>>({})
const settingsLoading = reactive<Record<number, boolean>>({})
const settingsSaving = reactive<Record<number, boolean>>({})
const settingsError = reactive<Record<number, string>>({})
const settingsSuccess = reactive<Record<number, boolean>>({})

// Editable form state per key
const editSource = reactive<Record<number, string>>({})
const editLang = reactive<Record<number, string>>({})
const editTextless = reactive<Record<number, boolean>>({})

async function toggleSettings(id: number) {
  if (expandedKey.value === id) {
    expandedKey.value = null
    return
  }
  expandedKey.value = id
  if (!keySettings[id]) {
    await loadSettings(id)
  }
}

async function loadSettings(id: number) {
  settingsLoading[id] = true
  settingsError[id] = ''
  try {
    const res = await keysApi.getSettings(id)
    if (res.ok) {
      const data: KeySettings = await res.json()
      keySettings[id] = data
      editSource[id] = data.poster_source
      editLang[id] = data.fanart_lang
      editTextless[id] = data.fanart_textless
    } else {
      settingsError[id] = 'Failed to load settings'
    }
  } catch {
    settingsError[id] = 'Failed to load settings'
  } finally {
    settingsLoading[id] = false
  }
}

async function saveSettings(id: number) {
  if (settingsSaving[id]) return
  settingsSaving[id] = true
  settingsError[id] = ''
  settingsSuccess[id] = false
  try {
    const res = await keysApi.updateSettings(id, {
      poster_source: editSource[id] ?? 'tmdb',
      fanart_lang: editLang[id] ?? 'en',
      fanart_textless: editTextless[id] ?? false,
    })
    if (res.ok) {
      settingsSuccess[id] = true
      await loadSettings(id)
      setTimeout(() => (settingsSuccess[id] = false), 2000)
    } else {
      const data = await res.json().catch(() => null)
      settingsError[id] = data?.error || 'Failed to save'
    }
  } catch {
    settingsError[id] = 'Failed to save'
  } finally {
    settingsSaving[id] = false
  }
}

async function resetSettings(id: number) {
  settingsSaving[id] = true
  settingsError[id] = ''
  try {
    const res = await keysApi.deleteSettings(id)
    if (res.ok) {
      await loadSettings(id)
    } else {
      settingsError[id] = 'Failed to reset'
    }
  } catch {
    settingsError[id] = 'Failed to reset'
  } finally {
    settingsSaving[id] = false
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
        <div v-if="expandedKey === key.id" class="border-t px-3 py-4 space-y-4 bg-muted/30">
          <div v-if="settingsLoading[key.id]" class="text-sm text-muted-foreground">Loading settings...</div>
          <template v-else-if="keySettings[key.id]">
            <div class="flex items-center gap-2">
              <h3 class="text-sm font-semibold">Poster Settings</h3>
              <span
                v-if="keySettings[key.id]?.is_default"
                class="text-xs bg-secondary text-secondary-foreground px-2 py-0.5 rounded"
              >
                Using defaults
              </span>
            </div>

            <div class="space-y-2">
              <label class="text-sm font-medium">Poster Source</label>
              <select
                v-model="editSource[key.id]"
                class="flex h-9 w-full max-w-xs rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              >
                <option value="tmdb">TMDB</option>
                <option value="fanart" :disabled="!keySettings[key.id]?.fanart_available">
                  Fanart.tv{{ !keySettings[key.id]?.fanart_available ? ' (no API key)' : '' }}
                </option>
              </select>
            </div>

            <template v-if="editSource[key.id] === 'fanart'">
              <div class="space-y-2">
                <label class="text-sm font-medium">Language</label>
                <Input
                  v-model="editLang[key.id]"
                  type="text"
                  placeholder="en"
                  class="max-w-[120px]"
                />
              </div>

              <div class="flex items-center gap-2">
                <input
                  :id="`textless-${key.id}`"
                  v-model="editTextless[key.id]"
                  type="checkbox"
                  class="h-4 w-4 rounded border-input"
                />
                <label :for="`textless-${key.id}`" class="text-sm font-medium">Prefer textless posters</label>
              </div>
            </template>

            <div class="flex items-center gap-3 pt-1">
              <Button size="sm" :disabled="settingsSaving[key.id]" @click="saveSettings(key.id)">
                {{ settingsSaving[key.id] ? 'Saving...' : 'Save' }}
              </Button>
              <Button
                v-if="!keySettings[key.id]?.is_default"
                variant="outline"
                size="sm"
                :disabled="settingsSaving[key.id]"
                @click="resetSettings(key.id)"
              >
                Reset to defaults
              </Button>
              <span v-if="settingsSuccess[key.id]" class="text-sm text-green-600">Saved</span>
              <span v-if="settingsError[key.id]" class="text-sm text-destructive">{{ settingsError[key.id] }}</span>
            </div>
          </template>
        </div>
      </div>
    </div>
  </div>
</template>
