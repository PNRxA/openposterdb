import { createRouter, createWebHistory, type RouteLocationNormalized } from 'vue-router'
import { useAuthStore } from '@/stores/auth'

export async function routeGuard(to: RouteLocationNormalized) {
  const auth = useAuthStore()

  // Only check setup status when navigating to setup or login (avoids an API
  // call on every navigation). The result is cached in the auth store after the
  // first successful call.
  if (to.name === 'setup' || to.name === 'login' || to.name === 'landing' || to.name === 'dashboard') {
    try {
      const setupRequired = await auth.checkSetupRequired()
      if (setupRequired && to.name !== 'setup') {
        auth.logout()
        return { name: 'setup' }
      }
      if (!setupRequired && to.name === 'setup') {
        return { name: 'login' }
      }
    } catch {
      // If we can't check, continue
    }
  }

  // Authenticated users hitting landing → redirect to admin dashboard
  if (to.name === 'landing') {
    if (auth.isAdminSession) {
      return { name: 'dashboard' }
    }
    if (auth.isApiKeySession) {
      return { name: 'key-settings' }
    }
  }

  // Admin routes require admin session (not API key session)
  if (to.matched.some((r) => r.meta.requiresAuth)) {
    if (!auth.isAdminSession) {
      if (auth.isApiKeySession) {
        return { name: 'key-settings' }
      }
      return { name: 'login' }
    }
  }

  // API key routes require API key session
  if (to.matched.some((r) => r.meta.requiresApiKey) && !auth.isApiKeySession) {
    return { name: 'login' }
  }

  // Redirect away from login/setup if already authenticated
  if (to.name === 'login' || to.name === 'setup') {
    if (auth.isAdminSession) {
      return { name: 'dashboard' }
    }
    if (auth.isApiKeySession) {
      return { name: 'key-settings' }
    }
  }
}

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    {
      path: '/',
      name: 'landing',
      component: () => import('@/views/LandingView.vue'),
    },
    {
      path: '/admin',
      component: () => import('@/layouts/DashboardLayout.vue'),
      meta: { requiresAuth: true },
      children: [
        {
          path: '',
          name: 'dashboard',
          component: () => import('@/views/DashboardView.vue'),
          meta: { title: 'Dashboard' },
        },
        {
          path: 'posters',
          name: 'posters',
          component: () => import('@/views/PostersView.vue'),
          meta: { title: 'Posters' },
        },
        {
          path: 'logos',
          name: 'logos',
          component: () => import('@/views/LogosView.vue'),
          meta: { title: 'Logos' },
        },
        {
          path: 'backdrops',
          name: 'backdrops',
          component: () => import('@/views/BackdropsView.vue'),
          meta: { title: 'Backdrops' },
        },
        {
          path: 'keys',
          name: 'keys',
          component: () => import('@/views/ApiKeysView.vue'),
          meta: { title: 'API Keys' },
        },
        {
          path: 'settings',
          name: 'settings',
          component: () => import('@/views/SettingsView.vue'),
          meta: { title: 'Settings' },
        },
      ],
    },
    {
      path: '/key-settings',
      name: 'key-settings',
      component: () => import('@/views/KeySettingsView.vue'),
      meta: { requiresApiKey: true },
    },
    {
      path: '/login',
      name: 'login',
      component: () => import('@/views/LoginView.vue'),
    },
    {
      path: '/setup',
      name: 'setup',
      component: () => import('@/views/SetupView.vue'),
    },
    {
      path: '/:pathMatch(.*)*',
      redirect: '/',
    },
  ],
})

router.beforeEach(routeGuard)

export default router
