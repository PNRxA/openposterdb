<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import { get, post, del } from '@/lib/api'
import { Button } from '@/components/ui/button'

const auth = useAuthStore()
const router = useRouter()

interface ApiKey {
  id: number
  name: string
  key_prefix: string
  created_at: string
  last_used_at: string | null
}

const keys = ref<ApiKey[]>([])
const newKeyName = ref('')
const newKeyValue = ref<string | null>(null)
const error = ref('')
const loading = ref(false)

async function loadKeys() {
  const res = await get('/api/keys')
  if (res.ok) {
    keys.value = await res.json()
  }
}

async function createKey() {
  if (!newKeyName.value.trim()) return
  error.value = ''
  loading.value = true
  try {
    const res = await post('/api/keys', { name: newKeyName.value.trim() })
    if (res.ok) {
      const data = await res.json()
      newKeyValue.value = data.key
      newKeyName.value = ''
      await loadKeys()
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
  await del(`/api/keys/${id}`)
  await loadKeys()
}

function handleLogout() {
  auth.logout()
  router.push('/login')
}

onMounted(loadKeys)
</script>

<template>
  <div class="min-h-screen p-8 max-w-3xl mx-auto space-y-8">
    <div class="flex items-center justify-between">
      <h1 class="text-2xl font-bold">API Keys</h1>
      <Button variant="outline" @click="handleLogout">Sign out</Button>
    </div>

    <!-- Create new key -->
    <div class="space-y-3">
      <h2 class="text-lg font-semibold">Create new key</h2>
      <form class="flex gap-2" @submit.prevent="createKey">
        <input
          v-model="newKeyName"
          type="text"
          placeholder="Key name (e.g. jellyfin-prod)"
          required
          class="flex-1 rounded-md border border-input bg-background px-3 py-2 text-sm"
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
      <div v-for="key in keys" :key="key.id" class="flex items-center justify-between rounded-md border p-3">
        <div class="space-y-1">
          <p class="font-medium text-sm">{{ key.name }}</p>
          <p class="text-xs text-muted-foreground">
            <span class="font-mono">{{ key.key_prefix }}...</span>
            &middot; Created {{ key.created_at }}
            <template v-if="key.last_used_at"> &middot; Last used {{ key.last_used_at }}</template>
          </p>
        </div>
        <Button variant="destructive" size="sm" @click="deleteKey(key.id)">Delete</Button>
      </div>
    </div>
  </div>
</template>
