import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import ClearCacheButton from '@/components/ClearCacheButton.vue'

const mockAdminApi = vi.hoisted(() => ({ purgeAll: vi.fn() }))
vi.mock('@/lib/api', () => ({ adminApi: mockAdminApi }))

function mountButton() {
  return mount(ClearCacheButton, {
    global: {
      stubs: {
        Button: {
          template: '<button :disabled="disabled" @click="$emit(\'click\')"><slot /></button>',
          props: ['disabled', 'variant', 'size'],
          emits: ['click'],
        },
        Dialog: { template: '<div v-if="open"><slot /></div>', props: ['open'] },
        DialogContent: { template: '<div><slot /></div>' },
        DialogHeader: { template: '<div><slot /></div>' },
        DialogTitle: { template: '<div><slot /></div>' },
        Trash2: { template: '<span />' },
        Loader2: { template: '<span />' },
      },
    },
  })
}

async function openAndConfirm(wrapper: ReturnType<typeof mountButton>) {
  // Only the trigger is shown until clicked (dialog is closed).
  await wrapper.find('button').trigger('click')
  await flushPromises()
  const clearButtons = wrapper.findAll('button').filter((b) => b.text().trim() === 'Clear cache')
  await clearButtons[clearButtons.length - 1]!.trigger('click') // the dialog confirm
  await flushPromises()
}

describe('ClearCacheButton', () => {
  beforeEach(() => vi.clearAllMocks())

  it('does not show the dialog until the button is clicked', () => {
    mockAdminApi.purgeAll.mockReturnValue(new Promise(() => {}))
    const wrapper = mountButton()
    expect(wrapper.findAll('button').length).toBe(1)
  })

  it('clears the cache and emits a summary message', async () => {
    mockAdminApi.purgeAll.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ ok: true, external_cache_only: false, dirs_cleared: 4, meta_deleted: 42, ratings_deleted: 5 }),
    })
    const wrapper = mountButton()

    await openAndConfirm(wrapper)

    expect(mockAdminApi.purgeAll).toHaveBeenCalledTimes(1)
    const cleared = wrapper.emitted('cleared')
    expect(cleared).toBeTruthy()
    expect(cleared![0]![0]).toContain('Cache cleared')
    expect(cleared![0]![0]).toContain('42')
  })

  it('emits the partial-purge message under external cache only', async () => {
    mockAdminApi.purgeAll.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ ok: true, external_cache_only: true, dirs_cleared: 0, meta_deleted: 0, ratings_deleted: 0 }),
    })
    const wrapper = mountButton()

    await openAndConfirm(wrapper)

    expect(wrapper.emitted('cleared')![0]![0]).toContain('external CDN')
  })

  it('shows an error and does not emit cleared on failure', async () => {
    mockAdminApi.purgeAll.mockResolvedValue({
      ok: false,
      status: 500,
      text: () => Promise.resolve(JSON.stringify({ error: 'boom' })),
    })
    const wrapper = mountButton()

    await openAndConfirm(wrapper)

    expect(wrapper.emitted('cleared')).toBeFalsy()
    expect(wrapper.text()).toContain('boom')
  })
})
