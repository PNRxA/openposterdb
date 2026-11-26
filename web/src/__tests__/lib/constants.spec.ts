import { describe, it, expect } from 'vitest'
import {
  ALL_RATING_SOURCES,
  DEFAULT_RATINGS_ORDER,
  parseRatingsOrder,
  parseRatingsExclude,
  BADGE_STYLE_LABELS,
  BADGE_DIRECTION_LABELS,
  LABEL_STYLE_LABELS,
  BADGE_SIZE_LABELS,
  FONT_SIZE_LABELS,
  BADGE_SHAPE_LABELS,
  BADGE_BACKGROUND_LABELS,
  IMAGE_SOURCE_LABELS,
  POSITION_LABELS,
} from '@/lib/constants'

describe('ALL_RATING_SOURCES', () => {
  it('has 10 rating sources', () => {
    expect(ALL_RATING_SOURCES).toHaveLength(10)
  })

  it('each source has key, label, and color', () => {
    for (const source of ALL_RATING_SOURCES) {
      expect(source.key).toBeTruthy()
      expect(source.label).toBeTruthy()
      expect(source.color).toMatch(/^#[0-9a-f]{6}$/i)
    }
  })

  it('keys are unique', () => {
    const keys = ALL_RATING_SOURCES.map(s => s.key)
    expect(new Set(keys).size).toBe(keys.length)
  })
})

describe('parseRatingsOrder', () => {
  it('returns keys in given order followed by missing keys', () => {
    const result = parseRatingsOrder('imdb,tmdb')
    expect(result[0]).toBe('imdb')
    expect(result[1]).toBe('tmdb')
    // All 10 keys should be present
    expect(result).toHaveLength(10)
  })

  it('appends missing keys for partial input', () => {
    const result = parseRatingsOrder('rt')
    expect(result[0]).toBe('rt')
    // All other keys should follow
    const allKeys = ALL_RATING_SOURCES.map(s => s.key)
    for (const key of allKeys) {
      expect(result).toContain(key)
    }
  })

  it('handles empty string by returning all keys', () => {
    const result = parseRatingsOrder('')
    const allKeys = ALL_RATING_SOURCES.map(s => s.key)
    expect(result).toHaveLength(allKeys.length)
    for (const key of allKeys) {
      expect(result).toContain(key)
    }
  })

  it('preserves the full default order', () => {
    const result = parseRatingsOrder(DEFAULT_RATINGS_ORDER)
    expect(result).toEqual(DEFAULT_RATINGS_ORDER.split(','))
  })

  it('trims whitespace around keys', () => {
    const result = parseRatingsOrder('imdb , tmdb')
    expect(result[0]).toBe('imdb')
    expect(result[1]).toBe('tmdb')
  })

  it('does not include a key twice when already present in input', () => {
    // parseRatingsOrder appends missing keys — keys already in the input should not be appended again
    const result = parseRatingsOrder('imdb,tmdb')
    const imdbCount = result.filter((k: string) => k === 'imdb').length
    expect(imdbCount).toBe(1)
  })
})

describe('parseRatingsExclude', () => {
  it('returns empty array for empty string', () => {
    expect(parseRatingsExclude('')).toEqual([])
  })

  it('returns only the explicitly listed keys (does NOT append missing)', () => {
    expect(parseRatingsExclude('rt,trakt')).toEqual(['rt', 'trakt'])
  })

  it('trims whitespace around keys', () => {
    expect(parseRatingsExclude(' rt , mc ')).toEqual(['rt', 'mc'])
  })

  it('drops unknown keys', () => {
    expect(parseRatingsExclude('rt,bogus,mal')).toEqual(['rt', 'mal'])
  })
})

describe('enum code → label maps', () => {
  it('badge style/direction map the h/v/d codes', () => {
    for (const map of [BADGE_STYLE_LABELS, BADGE_DIRECTION_LABELS]) {
      expect(map.h).toBe('Horizontal')
      expect(map.v).toBe('Vertical')
      expect(map.d).toBe('Auto')
    }
  })

  it('label style maps the t/i/o codes', () => {
    expect(LABEL_STYLE_LABELS).toEqual({ t: 'Text', i: 'Icon', o: 'Official' })
  })

  it('badge size maps every xs–xl code', () => {
    expect(Object.keys(BADGE_SIZE_LABELS).sort()).toEqual(['l', 'm', 's', 'xl', 'xs'])
    expect(BADGE_SIZE_LABELS.m).toBe('Medium')
  })

  it('font size maps every xs–xl code like badge size', () => {
    expect(FONT_SIZE_LABELS).toEqual(BADGE_SIZE_LABELS)
    expect(FONT_SIZE_LABELS.m).toBe('Medium')
  })

  it('badge shape maps the r/p codes', () => {
    expect(BADGE_SHAPE_LABELS).toEqual({ r: 'Rounded', p: 'Pill' })
  })

  it('badge background maps the d/k/t/n codes', () => {
    expect(BADGE_BACKGROUND_LABELS).toEqual({ d: 'Default', k: 'Dark', t: 'Transparent', n: 'None' })
  })

  it('image source maps the t/f codes', () => {
    expect(IMAGE_SOURCE_LABELS).toEqual({ t: 'TMDB', f: 'Fanart.tv' })
  })

  it('position maps all eight placement codes', () => {
    expect(Object.keys(POSITION_LABELS)).toHaveLength(8)
    expect(POSITION_LABELS.bc).toBe('Bottom Center')
    expect(POSITION_LABELS.tr).toBe('Top Right')
  })
})
