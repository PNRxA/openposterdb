import { describe, it, expect, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import LandingView from '@/views/LandingView.vue'

function mountView() {
  return mount(LandingView, {
    global: {
      plugins: [createPinia()],
      stubs: {
        Button: { template: '<button><slot /></button>', props: ['variant', 'size', 'asChild'] },
        Card: { template: '<div><slot /></div>' },
        CardHeader: { template: '<div><slot /></div>' },
        CardTitle: { template: '<div><slot /></div>' },
        CardContent: { template: '<div><slot /></div>' },
        Image: { template: '<span />' },
        KeyRound: { template: '<span />' },
        Zap: { template: '<span />' },
        Shield: { template: '<span />' },
        Github: { template: '<span />' },
        FreeApiKeyCard: { template: '<div data-testid="free-api-key-card" />' },
        'router-link': { template: '<a><slot /></a>', props: ['to'] },
      },
    },
  })
}

describe('LandingView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('renders the title and description', () => {
    const wrapper = mountView()
    expect(wrapper.text()).toContain('OpenPosterDB')
    expect(wrapper.text()).toContain('Self-hosted poster, logo, and backdrop serving')
  })

  it('renders feature cards', () => {
    const wrapper = mountView()
    expect(wrapper.text()).toContain('Posters & Backdrops')
    expect(wrapper.text()).toContain('API Key Management')
    expect(wrapper.text()).toContain('Fast & Cached')
    expect(wrapper.text()).toContain('RPDB Compatible')
  })

  it('has a sign in link', () => {
    const wrapper = mountView()
    expect(wrapper.text()).toContain('Sign in')
  })

  it('has a GitHub link', () => {
    const wrapper = mountView()
    expect(wrapper.text()).toContain('GitHub')
  })

  it('includes FreeApiKeyCard component', () => {
    const wrapper = mountView()
    expect(wrapper.find('[data-testid="free-api-key-card"]').exists()).toBe(true)
  })
})
