import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { authApi } from '@/lib/auth-api'

export const useAuthStore = defineStore('auth', () => {
  const token = ref<string | null>(localStorage.getItem('token'))
  const setupRequired = ref<boolean | null>(null)

  // API key session state (sessionStorage so it clears on tab close)
  const apiKeyToken = ref<string | null>(sessionStorage.getItem('apiKeyToken'))
  const apiKeyInfo = ref<{ name: string; key_prefix: string } | null>(null)

  const isAdminSession = computed(() => !!token.value)
  const isApiKeySession = computed(() => !!apiKeyToken.value)
  const isAuthenticated = computed(() => isAdminSession.value || isApiKeySession.value)

  function setToken(t: string) {
    token.value = t
    localStorage.setItem('token', t)
  }

  function clearToken() {
    token.value = null
    localStorage.removeItem('token')
  }

  async function checkSetupRequired(): Promise<boolean> {
    if (setupRequired.value !== null) return setupRequired.value
    const res = await authApi.status()
    if (!res.ok) throw new Error(`status check failed: ${res.status}`)
    const data = await res.json()
    setupRequired.value = data.setup_required
    return data.setup_required
  }

  async function setup(username: string, password: string): Promise<boolean> {
    const res = await authApi.setup(username, password)
    if (!res.ok) return false
    const data = await res.json()
    setToken(data.token)
    setupRequired.value = false
    return true
  }

  async function login(username: string, password: string): Promise<boolean> {
    const res = await authApi.login(username, password)
    if (!res.ok) return false
    const data = await res.json()
    setToken(data.token)
    return true
  }

  async function loginWithApiKey(key: string): Promise<boolean> {
    const res = await authApi.keyLogin(key)
    if (!res.ok) return false
    const data = await res.json()
    apiKeyToken.value = data.token
    sessionStorage.setItem('apiKeyToken', data.token)
    apiKeyInfo.value = { name: data.name, key_prefix: data.key_prefix }
    return true
  }

  function logoutApiKey() {
    apiKeyToken.value = null
    apiKeyInfo.value = null
    sessionStorage.removeItem('apiKeyToken')
  }

  async function refresh(): Promise<boolean> {
    const res = await authApi.refresh()
    if (!res.ok) return false
    const data = await res.json()
    setToken(data.token)
    return true
  }

  function logout() {
    clearToken()
    logoutApiKey()
    setupRequired.value = null
  }

  return {
    token,
    apiKeyToken,
    apiKeyInfo,
    isAuthenticated,
    isAdminSession,
    isApiKeySession,
    setupRequired,
    checkSetupRequired,
    setup,
    login,
    loginWithApiKey,
    logoutApiKey,
    refresh,
    logout,
  }
})
