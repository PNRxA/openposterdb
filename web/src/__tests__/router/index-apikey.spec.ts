import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { createRouter, createWebHistory } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import { routeGuard } from '@/router/index'

function makeRouter() {
  const router = createRouter({
    history: createWebHistory(),
    routes: [
      {
        path: '/',
        name: 'landing',
        component: { template: '<div>Landing</div>' },
      },
      {
        path: '/admin',
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
  router.beforeEach(routeGuard)
  return router
}

describe('router - API key session', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('API key session visiting / redirects to /key-settings', async () => {
    const router = makeRouter()
    const auth = useAuthStore()
    auth.apiKeyToken = 'jwt-token'

    await router.push('/')
    await router.isReady()

    expect(router.currentRoute.value.name).toBe('key-settings')
  })

  it('API key session visiting /admin redirects to /key-settings', async () => {
    const router = makeRouter()
    const auth = useAuthStore()
    auth.apiKeyToken = 'jwt-token'

    await router.push('/admin')
    await router.isReady()

    expect(router.currentRoute.value.name).toBe('key-settings')
  })

  it('API key session visiting /login redirects to /key-settings', async () => {
    const router = makeRouter()
    const auth = useAuthStore()
    auth.apiKeyToken = 'jwt-token'

    await router.push('/login')
    await router.isReady()

    expect(router.currentRoute.value.name).toBe('key-settings')
  })

  it('unauthenticated user visiting /key-settings redirects to /login', async () => {
    const router = makeRouter()
    const auth = useAuthStore()

    await router.push('/key-settings')
    await router.isReady()

    expect(router.currentRoute.value.name).toBe('login')
  })

  it('admin session can access /admin but not /key-settings', async () => {
    const router = makeRouter()
    const auth = useAuthStore()
    auth.token = 'jwt-token'

    await router.push('/admin')
    await router.isReady()
    expect(router.currentRoute.value.name).toBe('dashboard')

    // Admin without apiKey gets bounced from /key-settings → login → dashboard
    await router.push('/key-settings')
    await router.isReady()
    expect(router.currentRoute.value.name).toBe('dashboard')
  })
})
