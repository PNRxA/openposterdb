import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { createRouter, createWebHistory } from 'vue-router'
import { useAuthStore } from '@/stores/auth'

function makeRouter() {
  return createRouter({
    history: createWebHistory(),
    routes: [
      {
        path: '/',
        component: { template: '<router-view />' },
        meta: { requiresAuth: true },
        children: [
          { path: '', name: 'dashboard', component: { template: '<div>Dashboard</div>' } },
          { path: 'keys', name: 'keys', component: { template: '<div>Keys</div>' } },
        ],
      },
      {
        path: '/key-settings',
        name: 'key-settings',
        component: { template: '<div>KeySettings</div>' },
        meta: { requiresApiKey: true },
      },
      { path: '/login', name: 'login', component: { template: '<div>Login</div>' } },
      { path: '/setup', name: 'setup', component: { template: '<div>Setup</div>' } },
    ],
  })
}

// Replicate the actual guard logic for testing
function addGuard(router: ReturnType<typeof makeRouter>, auth: ReturnType<typeof useAuthStore>) {
  router.beforeEach(async (to) => {
    if (to.matched.some((r) => r.meta.requiresAuth)) {
      if (!auth.isAdminSession) {
        if (auth.isApiKeySession) {
          return { name: 'key-settings' }
        }
        return { name: 'login' }
      }
    }
    if (to.matched.some((r) => r.meta.requiresApiKey) && !auth.isApiKeySession) {
      return { name: 'login' }
    }
    if (to.name === 'login') {
      if (auth.isAdminSession) return { name: 'dashboard' }
      if (auth.isApiKeySession) return { name: 'key-settings' }
    }
  })
}

describe('router - API key session', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('API key session visiting / redirects to /key-settings', async () => {
    const router = makeRouter()
    const auth = useAuthStore()
    auth.apiKeyToken = 'jwt-token'

    addGuard(router, auth)

    await router.push('/')
    await router.isReady()

    expect(router.currentRoute.value.name).toBe('key-settings')
  })

  it('API key session visiting /login redirects to /key-settings', async () => {
    const router = makeRouter()
    const auth = useAuthStore()
    auth.apiKeyToken = 'jwt-token'

    addGuard(router, auth)

    await router.push('/login')
    await router.isReady()

    expect(router.currentRoute.value.name).toBe('key-settings')
  })

  it('unauthenticated user visiting /key-settings redirects to /login', async () => {
    const router = makeRouter()
    const auth = useAuthStore()

    addGuard(router, auth)

    await router.push('/key-settings')
    await router.isReady()

    expect(router.currentRoute.value.name).toBe('login')
  })

  it('admin session can access / but not /key-settings', async () => {
    const router = makeRouter()
    const auth = useAuthStore()
    auth.token = 'jwt-token'

    addGuard(router, auth)

    await router.push('/')
    await router.isReady()
    expect(router.currentRoute.value.name).toBe('dashboard')

    // Admin without apiKey gets bounced from /key-settings → login → dashboard
    await router.push('/key-settings')
    await router.isReady()
    expect(router.currentRoute.value.name).toBe('dashboard')
  })
})
