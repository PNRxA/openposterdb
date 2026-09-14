import { describe, it, expect } from 'vitest'
import { titleIdFromCacheValue } from '@/lib/utils'

describe('titleIdFromCacheValue', () => {
  it('strips the ratings suffix (no variant)', () => {
    expect(titleIdFromCacheValue('tt0111161@imc')).toBe('tt0111161')
    expect(titleIdFromCacheValue('episode-1396-S1E1@i')).toBe('episode-1396-S1E1')
  })

  it('strips a language/textless or kind variant', () => {
    expect(titleIdFromCacheValue('tt0111161_t_de@imc')).toBe('tt0111161')
    expect(titleIdFromCacheValue('tt0111161_f_tl@imc')).toBe('tt0111161')
    expect(titleIdFromCacheValue('tt0111161_l_t_en@i')).toBe('tt0111161')
  })

  it('keeps hyphenated tmdb ids intact', () => {
    expect(titleIdFromCacheValue('movie-12345@i')).toBe('movie-12345')
    expect(titleIdFromCacheValue('series-99@i.p1')).toBe('series-99')
  })

  it('returns the value unchanged when there is no delimiter', () => {
    expect(titleIdFromCacheValue('tt0111161')).toBe('tt0111161')
  })
})
