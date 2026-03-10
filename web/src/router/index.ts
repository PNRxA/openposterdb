import { createRouter, createWebHistory } from 'vue-router'
import { useAuthStore } from '@/stores/auth'

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    {
      path: '/',
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
      path: '/login',
      name: 'login',
      component: () => import('@/views/LoginView.vue'),
    },
    {
      path: '/setup',
      name: 'setup',
      component: () => import('@/views/SetupView.vue'),
    },
  ],
})

router.beforeEach(async (to) => {
  const auth = useAuthStore()

  // Check if setup is needed
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

  // Auth guard — check the matched route chain for requiresAuth
  if (to.matched.some((r) => r.meta.requiresAuth) && !auth.isAuthenticated) {
    return { name: 'login' }
  }

  // Redirect away from login/setup if already authenticated
  if ((to.name === 'login' || to.name === 'setup') && auth.isAuthenticated) {
    return { name: 'dashboard' }
  }
})

export default router
