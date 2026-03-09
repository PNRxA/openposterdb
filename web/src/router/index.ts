import { createRouter, createWebHistory } from 'vue-router'
import { useAuthStore } from '@/stores/auth'

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    {
      path: '/',
      redirect: '/keys',
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
      path: '/keys',
      name: 'keys',
      component: () => import('@/views/ApiKeysView.vue'),
      meta: { requiresAuth: true },
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

  // Auth guard
  if (to.meta.requiresAuth && !auth.isAuthenticated) {
    return { name: 'login' }
  }

  // Redirect away from login/setup if already authenticated
  if ((to.name === 'login' || to.name === 'setup') && auth.isAuthenticated) {
    return { name: 'keys' }
  }
})

export default router
