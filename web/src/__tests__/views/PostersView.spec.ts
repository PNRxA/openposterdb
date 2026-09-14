import { describe, it, expect, vi } from 'vitest'
import { shallowMount } from '@vue/test-utils'
import PostersView from '@/views/PostersView.vue'
import ImageListView from '@/components/ImageListView.vue'
import { adminApi } from '@/lib/api'

vi.mock('@/lib/api', () => ({
  adminApi: {
    getPosters: vi.fn(),
    getPosterImage: vi.fn(),
    fetchPoster: vi.fn(),
    purgePoster: vi.fn(),
    clearPosters: vi.fn(),
  },
}))

describe('PostersView', () => {
  it('renders ImageListView with kind="poster"', () => {
    const wrapper = shallowMount(PostersView)

    const imageList = wrapper.findComponent(ImageListView)
    expect(imageList.exists()).toBe(true)
    expect(imageList.props('kind')).toBe('poster')
  })

  it('passes correct API functions as props', () => {
    const wrapper = shallowMount(PostersView)

    const imageList = wrapper.findComponent(ImageListView)
    expect(imageList.props('listFn')).toBe(adminApi.getPosters)
    expect(imageList.props('imageFn')).toBe(adminApi.getPosterImage)
    expect(imageList.props('fetchFn')).toBe(adminApi.fetchPoster)
    expect(imageList.props('deleteFn')).toBe(adminApi.purgePoster)
    expect(imageList.props('clearAllFn')).toBe(adminApi.clearPosters)
  })
})
