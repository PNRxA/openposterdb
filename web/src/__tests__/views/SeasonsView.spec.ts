import { describe, it, expect, vi } from 'vitest'
import { shallowMount } from '@vue/test-utils'
import SeasonsView from '@/views/SeasonsView.vue'
import ImageListView from '@/components/ImageListView.vue'
import { adminApi } from '@/lib/api'

vi.mock('@/lib/api', () => ({
  adminApi: {
    getSeasons: vi.fn(),
    getSeasonImage: vi.fn(),
    fetchSeason: vi.fn(),
  },
}))

describe('SeasonsView', () => {
  it('renders ImageListView with kind="season"', () => {
    const wrapper = shallowMount(SeasonsView)

    const imageList = wrapper.findComponent(ImageListView)
    expect(imageList.exists()).toBe(true)
    expect(imageList.props('kind')).toBe('season')
  })

  it('passes correct API functions as props', () => {
    const wrapper = shallowMount(SeasonsView)

    const imageList = wrapper.findComponent(ImageListView)
    expect(imageList.props('listFn')).toBe(adminApi.getSeasons)
    expect(imageList.props('imageFn')).toBe(adminApi.getSeasonImage)
    expect(imageList.props('fetchFn')).toBe(adminApi.fetchSeason)
  })
})
