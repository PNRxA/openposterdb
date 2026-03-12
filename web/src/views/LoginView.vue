<script setup lang="ts">
import { version } from '../../package.json'
import { ref, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import { Button } from '@/components/ui/button'
import FreeApiKeyCard from '@/components/FreeApiKeyCard.vue'

const auth = useAuthStore()
const router = useRouter()

const mode = ref<'admin' | 'apikey'>('admin')
const username = ref('')
const password = ref('')
const apiKeyInput = ref('')
const error = ref('')
const loading = ref(false)
const checking = ref(true)

onMounted(async () => {
  try {
    const needsSetup = await auth.checkSetupRequired()
    if (needsSetup) {
      router.replace('/setup')
      return
    }
  } catch {
    // API unreachable — stay on login
  }
  checking.value = false
})

async function handleLogin() {
  error.value = ''
  loading.value = true
  try {
    if (mode.value === 'admin') {
      const ok = await auth.login(username.value, password.value)
      if (ok) {
        router.push('/admin')
      } else {
        error.value = 'Invalid username or password'
      }
    } else {
      const ok = await auth.loginWithApiKey(apiKeyInput.value)
      if (ok) {
        router.push('/key-settings')
      } else {
        error.value = 'Invalid API key'
      }
    }
  } catch {
    error.value = 'Login failed'
  } finally {
    loading.value = false
  }
}

function toggleMode() {
  error.value = ''
  mode.value = mode.value === 'admin' ? 'apikey' : 'admin'
}
</script>

<template>
  <div v-if="!checking" class="min-h-screen flex items-center justify-center">
    <div class="w-full max-w-sm space-y-6">
      <div class="text-center">
        <h1 class="text-2xl font-bold">OpenPosterDB</h1>
        <p class="text-xs text-muted-foreground">v{{ version }}</p>
      </div>

      <form class="space-y-4" @submit.prevent="handleLogin">
        <template v-if="mode === 'admin'">
          <div>
            <label class="block text-sm font-medium mb-1" for="username">Username</label>
            <input
              id="username"
              v-model="username"
              type="text"
              autocomplete="username"
              required
              class="w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
            />
          </div>
          <div>
            <label class="block text-sm font-medium mb-1" for="password">Password</label>
            <input
              id="password"
              v-model="password"
              type="password"
              autocomplete="current-password"
              required
              class="w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
            />
          </div>
        </template>

        <template v-else>
          <div>
            <label class="block text-sm font-medium mb-1" for="apikey">API Key</label>
            <input
              id="apikey"
              v-model="apiKeyInput"
              type="password"
              autocomplete="off"
              required
              class="w-full rounded-md border border-input bg-background px-3 py-2 text-sm font-mono"
            />
          </div>
        </template>

        <p v-if="error" class="text-sm text-destructive">{{ error }}</p>
        <Button type="submit" class="w-full" :disabled="loading">
          {{ loading ? 'Signing in...' : 'Sign in' }}
        </Button>
      </form>

      <p class="text-center text-sm text-muted-foreground">
        <button type="button" class="underline hover:text-foreground" @click="toggleMode">
          {{ mode === 'admin' ? 'Sign in with API key instead' : 'Sign in as admin instead' }}
        </button>
      </p>

      <p class="text-center text-sm text-muted-foreground">
        <router-link to="/" class="underline hover:text-foreground">&larr; Back to home</router-link>
      </p>

      <FreeApiKeyCard />
    </div>
  </div>
</template>
