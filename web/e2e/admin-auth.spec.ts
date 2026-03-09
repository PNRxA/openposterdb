import { test, expect } from '@playwright/test'

test.describe('admin endpoints require authentication', () => {
  test('GET /api/admin/stats returns 401 without auth', async ({ request }) => {
    const res = await request.get('/api/admin/stats')
    expect(res.status()).toBe(401)
  })

  test('GET /api/admin/posters returns 401 without auth', async ({ request }) => {
    const res = await request.get('/api/admin/posters')
    expect(res.status()).toBe(401)
  })

  test('GET /api/admin/posters image returns 401 without auth', async ({ request }) => {
    const res = await request.get('/api/admin/posters/imdb/tt0111161/image')
    expect(res.status()).toBe(401)
  })

  test('GET /api/keys returns 401 without auth', async ({ request }) => {
    const res = await request.get('/api/keys')
    expect(res.status()).toBe(401)
  })

  test('POST /api/keys returns 401 without auth', async ({ request }) => {
    const res = await request.post('/api/keys', { data: { name: 'test' } })
    expect(res.status()).toBe(401)
  })

  test('DELETE /api/keys/1 returns 401 without auth', async ({ request }) => {
    const res = await request.delete('/api/keys/1')
    expect(res.status()).toBe(401)
  })

  test('POST /api/auth/logout returns 401 without auth', async ({ request }) => {
    const res = await request.post('/api/auth/logout')
    expect(res.status()).toBe(401)
  })
})
