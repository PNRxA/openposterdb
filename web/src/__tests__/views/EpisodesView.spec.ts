import { describe, it, expect, vi } from 'vitest'
import { shallowMount } from '@vue/test-utils'
import EpisodesView from '@/views/EpisodesView.vue'
import ImageListView from '@/components/ImageListView.vue'
import { adminApi } from '@/lib/api'

vi.mock('@/lib/api', () => ({
  adminApi: {
    getEpisodes: vi.fn(),
    getEpisodeImage: vi.fn(),
    fetchEpisode: vi.fn(),
    purgeEpisode: vi.fn(),
    clearEpisodes: vi.fn(),
  },
}))

describe('EpisodesView', () => {
  it('renders ImageListView with kind="episode"', () => {
    const wrapper = shallowMount(EpisodesView)

    const imageList = wrapper.findComponent(ImageListView)
    expect(imageList.exists()).toBe(true)
    expect(imageList.props('kind')).toBe('episode')
  })

  it('passes correct API functions as props', () => {
    const wrapper = shallowMount(EpisodesView)

    const imageList = wrapper.findComponent(ImageListView)
    expect(imageList.props('listFn')).toBe(adminApi.getEpisodes)
    expect(imageList.props('imageFn')).toBe(adminApi.getEpisodeImage)
    expect(imageList.props('fetchFn')).toBe(adminApi.fetchEpisode)
    expect(imageList.props('deleteFn')).toBe(adminApi.purgeEpisode)
    expect(imageList.props('clearAllFn')).toBe(adminApi.clearEpisodes)
  })
})
