import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import LoginView from '@/views/LoginView.vue'

const mockRouter = {
  push: vi.fn(),
  replace: vi.fn(),
}

const mockAuthStore = {
  checkSetupRequired: vi.fn().mockResolvedValue(false),
  login: vi.fn(),
  loginWithApiKey: vi.fn(),
  isAuthenticated: false,
  freeApiKeyEnabled: false,
}

vi.mock('vue-router', () => ({
  useRouter: () => mockRouter,
}))

vi.mock('@/stores/auth', () => ({
  useAuthStore: () => mockAuthStore,
}))

function mountView() {
  return mount(LoginView, {
    global: {
      plugins: [createPinia()],
      stubs: {
        Button: {
          template: '<button><slot /></button>',
          props: ['disabled'],
        },
        'router-link': {
          template: '<a><slot /></a>',
          props: ['to'],
        },
      },
    },
  })
}

describe('LoginView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    mockAuthStore.checkSetupRequired.mockResolvedValue(false)
    mockAuthStore.login.mockReset()
    mockAuthStore.loginWithApiKey.mockReset()
    mockAuthStore.freeApiKeyEnabled = false
  })

  it('renders admin login form by default', async () => {
    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.find('input#username').exists()).toBe(true)
    expect(wrapper.find('input#password').exists()).toBe(true)
    expect(wrapper.find('button[type="submit"]').exists()).toBe(true)
  })

  it('shows error message on failed admin login', async () => {
    mockAuthStore.login.mockResolvedValue(false)
    const wrapper = mountView()
    await flushPromises()

    await wrapper.find('input#username').setValue('user')
    await wrapper.find('input#password').setValue('wrong')
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(wrapper.text()).toContain('Invalid username or password')
  })

  it('calls auth.login on admin form submit', async () => {
    mockAuthStore.login.mockResolvedValue(true)
    const wrapper = mountView()
    await flushPromises()

    await wrapper.find('input#username').setValue('admin')
    await wrapper.find('input#password').setValue('secret')
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(mockAuthStore.login).toHaveBeenCalledWith('admin', 'secret')
    expect(mockRouter.push).toHaveBeenCalledWith('/admin')
  })

  it('toggles to API key mode', async () => {
    const wrapper = mountView()
    await flushPromises()

    // Click the toggle link
    await wrapper.find('button.underline').trigger('click')
    await flushPromises()

    // Should show API key input, not username/password
    expect(wrapper.find('input#apikey').exists()).toBe(true)
    expect(wrapper.find('input#username').exists()).toBe(false)
    expect(wrapper.find('input#password').exists()).toBe(false)
    expect(wrapper.text()).toContain('Sign in as admin instead')
  })

  it('calls loginWithApiKey in apikey mode', async () => {
    mockAuthStore.loginWithApiKey.mockResolvedValue(true)
    const wrapper = mountView()
    await flushPromises()

    // Switch to API key mode
    await wrapper.find('button.underline').trigger('click')
    await flushPromises()

    await wrapper.find('input#apikey').setValue('my-secret-key')
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(mockAuthStore.loginWithApiKey).toHaveBeenCalledWith('my-secret-key')
    expect(mockRouter.push).toHaveBeenCalledWith('/key-settings')
  })

  it('shows error on failed API key login', async () => {
    mockAuthStore.loginWithApiKey.mockResolvedValue(false)
    const wrapper = mountView()
    await flushPromises()

    await wrapper.find('button.underline').trigger('click')
    await flushPromises()

    await wrapper.find('input#apikey').setValue('bad-key')
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(wrapper.text()).toContain('Invalid API key')
  })

  it('shows free API key card when freeApiKeyEnabled is true', async () => {
    mockAuthStore.freeApiKeyEnabled = true
    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.text()).toContain('Free API Key Available')
    expect(wrapper.text()).toContain('t0-free-rpdb')
  })

  it('hides free API key card when freeApiKeyEnabled is false', async () => {
    mockAuthStore.freeApiKeyEnabled = false
    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.text()).not.toContain('Free API Key Available')
    expect(wrapper.text()).not.toContain('t0-free-rpdb')
  })

  it('toggles back to admin mode from apikey mode', async () => {
    const wrapper = mountView()
    await flushPromises()

    // Toggle to apikey
    await wrapper.find('button.underline').trigger('click')
    await flushPromises()
    expect(wrapper.find('input#apikey').exists()).toBe(true)

    // Toggle back to admin
    await wrapper.find('button.underline').trigger('click')
    await flushPromises()
    expect(wrapper.find('input#username').exists()).toBe(true)
    expect(wrapper.find('input#apikey').exists()).toBe(false)
    expect(wrapper.text()).toContain('Sign in with API key instead')
  })
})
