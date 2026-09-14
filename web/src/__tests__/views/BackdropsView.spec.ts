import { describe, it, expect, vi } from 'vitest'
import { shallowMount } from '@vue/test-utils'
import BackdropsView from '@/views/BackdropsView.vue'
import ImageListView from '@/components/ImageListView.vue'
import { adminApi } from '@/lib/api'

vi.mock('@/lib/api', () => ({
  adminApi: {
    getBackdrops: vi.fn(),
    getBackdropImage: vi.fn(),
    fetchBackdrop: vi.fn(),
    purgeBackdrop: vi.fn(),
    clearBackdrops: vi.fn(),
  },
}))

describe('BackdropsView', () => {
  it('renders ImageListView with kind="backdrop"', () => {
    const wrapper = shallowMount(BackdropsView)

    const imageList = wrapper.findComponent(ImageListView)
    expect(imageList.exists()).toBe(true)
    expect(imageList.props('kind')).toBe('backdrop')
  })

  it('passes correct API functions as props', () => {
    const wrapper = shallowMount(BackdropsView)

    const imageList = wrapper.findComponent(ImageListView)
    expect(imageList.props('listFn')).toBe(adminApi.getBackdrops)
    expect(imageList.props('imageFn')).toBe(adminApi.getBackdropImage)
    expect(imageList.props('fetchFn')).toBe(adminApi.fetchBackdrop)
    expect(imageList.props('deleteFn')).toBe(adminApi.purgeBackdrop)
    expect(imageList.props('clearAllFn')).toBe(adminApi.clearBackdrops)
  })
})
