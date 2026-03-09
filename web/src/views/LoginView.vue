<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import { Button } from '@/components/ui/button'

const auth = useAuthStore()
const router = useRouter()

const username = ref('')
const password = ref('')
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
    const ok = await auth.login(username.value, password.value)
    if (ok) {
      router.push('/keys')
    } else {
      error.value = 'Invalid username or password'
    }
  } catch {
    error.value = 'Login failed'
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <div v-if="!checking" class="min-h-screen flex items-center justify-center">
    <div class="w-full max-w-sm space-y-6">
      <div class="text-center">
        <h1 class="text-2xl font-bold">OpenPosterDB</h1>
        <p class="text-muted-foreground">Sign in to manage API keys</p>
      </div>

      <form class="space-y-4" @submit.prevent="handleLogin">
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
        <p v-if="error" class="text-sm text-destructive">{{ error }}</p>
        <Button type="submit" class="w-full" :disabled="loading">
          {{ loading ? 'Signing in...' : 'Sign in' }}
        </Button>
      </form>
    </div>
  </div>
</template>
