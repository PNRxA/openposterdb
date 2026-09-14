import { describe, it, expect, vi } from 'vitest'
import { shallowMount } from '@vue/test-utils'
import LogosView from '@/views/LogosView.vue'
import ImageListView from '@/components/ImageListView.vue'
import { adminApi } from '@/lib/api'

vi.mock('@/lib/api', () => ({
  adminApi: {
    getLogos: vi.fn(),
    getLogoImage: vi.fn(),
    fetchLogo: vi.fn(),
    purgeLogo: vi.fn(),
    clearLogos: vi.fn(),
  },
}))

describe('LogosView', () => {
  it('renders ImageListView with kind="logo"', () => {
    const wrapper = shallowMount(LogosView)

    const imageList = wrapper.findComponent(ImageListView)
    expect(imageList.exists()).toBe(true)
    expect(imageList.props('kind')).toBe('logo')
  })

  it('passes correct API functions as props', () => {
    const wrapper = shallowMount(LogosView)

    const imageList = wrapper.findComponent(ImageListView)
    expect(imageList.props('listFn')).toBe(adminApi.getLogos)
    expect(imageList.props('imageFn')).toBe(adminApi.getLogoImage)
    expect(imageList.props('fetchFn')).toBe(adminApi.fetchLogo)
    expect(imageList.props('deleteFn')).toBe(adminApi.purgeLogo)
    expect(imageList.props('clearAllFn')).toBe(adminApi.clearLogos)
  })
})
