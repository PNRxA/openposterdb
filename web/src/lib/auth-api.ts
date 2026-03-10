const BASE_URL = import.meta.env.VITE_API_URL || ''

export const authApi = {
  status: (): Promise<Response> =>
    fetch(`${BASE_URL}/api/auth/status`),
  setup: (username: string, password: string): Promise<Response> =>
    fetch(`${BASE_URL}/api/auth/setup`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username, password }),
      credentials: 'include',
    }),
  login: (username: string, password: string): Promise<Response> =>
    fetch(`${BASE_URL}/api/auth/login`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username, password }),
      credentials: 'include',
    }),
  refresh: (): Promise<Response> =>
    fetch(`${BASE_URL}/api/auth/refresh`, {
      method: 'POST',
      credentials: 'include',
    }),
  keyLogin: (apiKey: string): Promise<Response> =>
    fetch(`${BASE_URL}/api/auth/key-login`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ api_key: apiKey }),
    }),
}
