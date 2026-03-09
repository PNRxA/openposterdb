import { useAuthStore } from '@/stores/auth'

const BASE_URL = import.meta.env.VITE_API_URL || ''

async function request(path: string, options: RequestInit = {}): Promise<Response> {
  const auth = useAuthStore()
  const headers = new Headers(options.headers)

  if (auth.token) {
    headers.set('Authorization', `Bearer ${auth.token}`)
  }

  if (options.body && !headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json')
  }

  const res = await fetch(`${BASE_URL}${path}`, { ...options, headers, credentials: 'include' })

  if (res.status === 401 && auth.token) {
    // Try refreshing the token
    const refreshed = await auth.refresh()
    if (refreshed) {
      headers.set('Authorization', `Bearer ${auth.token}`)
      return fetch(`${BASE_URL}${path}`, { ...options, headers, credentials: 'include' })
    }
    auth.logout()
  }

  return res
}

export async function get(path: string): Promise<Response> {
  return request(path)
}

export async function post(path: string, body?: unknown): Promise<Response> {
  return request(path, {
    method: 'POST',
    body: body ? JSON.stringify(body) : undefined,
  })
}

export async function del(path: string): Promise<Response> {
  return request(path, { method: 'DELETE' })
}
