import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

const BASE_URL = import.meta.env.VITE_API_URL || ''

export const useAuthStore = defineStore('auth', () => {
  const token = ref<string | null>(localStorage.getItem('token'))
  const isAuthenticated = computed(() => !!token.value)
  const setupRequired = ref<boolean | null>(null)

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
    const res = await fetch(`${BASE_URL}/api/auth/status`)
    if (!res.ok) throw new Error(`status check failed: ${res.status}`)
    const data = await res.json()
    setupRequired.value = data.setup_required
    return data.setup_required
  }

  async function setup(username: string, password: string): Promise<boolean> {
    const res = await fetch(`${BASE_URL}/api/auth/setup`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username, password }),
      credentials: 'include',
    })
    if (!res.ok) return false
    const data = await res.json()
    setToken(data.token)
    setupRequired.value = false
    return true
  }

  async function login(username: string, password: string): Promise<boolean> {
    const res = await fetch(`${BASE_URL}/api/auth/login`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username, password }),
      credentials: 'include',
    })
    if (!res.ok) return false
    const data = await res.json()
    setToken(data.token)
    return true
  }

  async function refresh(): Promise<boolean> {
    const res = await fetch(`${BASE_URL}/api/auth/refresh`, {
      method: 'POST',
      credentials: 'include',
    })
    if (!res.ok) return false
    const data = await res.json()
    setToken(data.token)
    return true
  }

  function logout() {
    clearToken()
    setupRequired.value = null
  }

  return { token, isAuthenticated, setupRequired, checkSetupRequired, setup, login, refresh, logout }
})
