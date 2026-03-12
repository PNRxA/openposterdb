import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { createRouter, createWebHistory } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import { routeGuard } from '@/router/index'

vi.stubGlobal('sessionStorage', {
  getItem: vi.fn(() => null),
  setItem: vi.fn(),
  removeItem: vi.fn(),
})

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
          { path: 'posters', name: 'posters', component: { template: '<div>Posters</div>' } },
          { path: 'logos', name: 'logos', component: { template: '<div>Logos</div>' } },
          { path: 'backdrops', name: 'backdrops', component: { template: '<div>Backdrops</div>' } },
          { path: 'keys', name: 'keys', component: { template: '<div>Keys</div>' } },
        ],
      },
      {
        path: '/key-settings',
        name: 'key-settings',
        component: { template: '<div>Key Settings</div>' },
        meta: { requiresApiKey: true },
      },
      { path: '/login', name: 'login', component: { template: '<div>Login</div>' } },
      { path: '/setup', name: 'setup', component: { template: '<div>Setup</div>' } },
    ],
  })
  router.beforeEach(routeGuard)
  return router
}

describe('router', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('unauthenticated user visiting / sees landing page', async () => {
    const router = makeRouter()

    await router.push('/')
    await router.isReady()

    expect(router.currentRoute.value.name).toBe('landing')
  })

  it('unauthenticated user visiting /admin gets redirected to /login', async () => {
    const router = makeRouter()

    await router.push('/admin')
    await router.isReady()

    expect(router.currentRoute.value.name).toBe('login')
  })

  it('admin user visiting / gets redirected to dashboard', async () => {
    const router = makeRouter()
    const auth = useAuthStore()
    auth.token = 'valid-token'

    await router.push('/')
    await router.isReady()

    expect(router.currentRoute.value.name).toBe('dashboard')
  })

  it('admin user visiting /login gets redirected to dashboard', async () => {
    const router = makeRouter()
    const auth = useAuthStore()
    auth.token = 'valid-token'

    await router.push('/login')
    await router.isReady()

    expect(router.currentRoute.value.name).toBe('dashboard')
  })

  it('API key user visiting / gets redirected to key-settings', async () => {
    const router = makeRouter()
    const auth = useAuthStore()
    auth.apiKeyToken = 'jwt-token'

    await router.push('/')
    await router.isReady()

    expect(router.currentRoute.value.name).toBe('key-settings')
  })

  it('API key user visiting /login gets redirected to key-settings', async () => {
    const router = makeRouter()
    const auth = useAuthStore()
    auth.apiKeyToken = 'jwt-token'

    await router.push('/login')
    await router.isReady()

    expect(router.currentRoute.value.name).toBe('key-settings')
  })

  it('unauthenticated user visiting /key-settings gets redirected to /login', async () => {
    const router = makeRouter()

    await router.push('/key-settings')
    await router.isReady()

    expect(router.currentRoute.value.name).toBe('login')
  })

  it('API key user can access /key-settings', async () => {
    const router = makeRouter()
    const auth = useAuthStore()
    auth.apiKeyToken = 'jwt-token'

    await router.push('/key-settings')
    await router.isReady()

    expect(router.currentRoute.value.name).toBe('key-settings')
  })

  it('admin user can access admin routes', async () => {
    const router = makeRouter()
    const auth = useAuthStore()
    auth.token = 'valid-token'

    await router.push('/admin')
    await router.isReady()

    expect(router.currentRoute.value.name).toBe('dashboard')
  })
})
