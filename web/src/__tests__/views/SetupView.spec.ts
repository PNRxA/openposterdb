import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import SetupView from '@/views/SetupView.vue'

const mockRouter = {
  push: vi.fn(),
}

const mockAuthStore = {
  setup: vi.fn(),
}

vi.mock('vue-router', () => ({
  useRouter: () => mockRouter,
}))

vi.mock('@/stores/auth', () => ({
  useAuthStore: () => mockAuthStore,
}))

function mountView() {
  return mount(SetupView, {
    global: {
      plugins: [createPinia()],
      stubs: {
        Button: {
          template: '<button><slot /></button>',
          props: ['disabled'],
        },
      },
    },
  })
}

describe('SetupView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    mockAuthStore.setup.mockReset()
  })

  it('renders setup form with confirm password field', () => {
    const wrapper = mountView()

    expect(wrapper.find('input#username').exists()).toBe(true)
    expect(wrapper.find('input#password').exists()).toBe(true)
    expect(wrapper.find('input#confirm-password').exists()).toBe(true)
    expect(wrapper.find('button[type="submit"]').exists()).toBe(true)
  })

  it('shows error when passwords do not match', async () => {
    const wrapper = mountView()

    await wrapper.find('input#username').setValue('admin')
    await wrapper.find('input#password').setValue('password123')
    await wrapper.find('input#confirm-password').setValue('different')
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(wrapper.text()).toContain('Passwords do not match')
    expect(mockAuthStore.setup).not.toHaveBeenCalled()
  })

  it('shows error when password is less than 8 characters', async () => {
    const wrapper = mountView()

    await wrapper.find('input#username').setValue('admin')
    await wrapper.find('input#password').setValue('short')
    await wrapper.find('input#confirm-password').setValue('short')
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(wrapper.text()).toContain('Password must be at least 8 characters')
    expect(mockAuthStore.setup).not.toHaveBeenCalled()
  })

  it('calls auth.setup on valid submit', async () => {
    mockAuthStore.setup.mockResolvedValue(true)
    const wrapper = mountView()

    await wrapper.find('input#username').setValue('admin')
    await wrapper.find('input#password').setValue('password123')
    await wrapper.find('input#confirm-password').setValue('password123')
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(mockAuthStore.setup).toHaveBeenCalledWith('admin', 'password123')
    expect(mockRouter.push).toHaveBeenCalledWith('/admin')
  })
})
