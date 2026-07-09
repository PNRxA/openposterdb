export const FREE_API_KEY = 't0-free-rpdb'

export const LANGUAGES = [
  { code: 'ar', name: 'Arabic' },
  { code: 'bg', name: 'Bulgarian' },
  { code: 'ca', name: 'Catalan' },
  { code: 'zh', name: 'Chinese' },
  { code: 'zh-CN', name: 'Chinese (Simplified)' },
  { code: 'zh-TW', name: 'Chinese (Traditional)' },
  { code: 'hr', name: 'Croatian' },
  { code: 'cs', name: 'Czech' },
  { code: 'da', name: 'Danish' },
  { code: 'nl', name: 'Dutch' },
  { code: 'en', name: 'English' },
  { code: 'en-US', name: 'English (US)' },
  { code: 'en-GB', name: 'English (UK)' },
  { code: 'fi', name: 'Finnish' },
  { code: 'fr', name: 'French' },
  { code: 'fr-CA', name: 'French (Canada)' },
  { code: 'de', name: 'German' },
  { code: 'el', name: 'Greek' },
  { code: 'he', name: 'Hebrew' },
  { code: 'hi', name: 'Hindi' },
  { code: 'hu', name: 'Hungarian' },
  { code: 'id', name: 'Indonesian' },
  { code: 'it', name: 'Italian' },
  { code: 'ja', name: 'Japanese' },
  { code: 'ko', name: 'Korean' },
  { code: 'ms', name: 'Malay' },
  { code: 'no', name: 'Norwegian' },
  { code: 'fa', name: 'Persian' },
  { code: 'pl', name: 'Polish' },
  { code: 'pt', name: 'Portuguese' },
  { code: 'pt-BR', name: 'Portuguese (Brazil)' },
  { code: 'pt-PT', name: 'Portuguese (Portugal)' },
  { code: 'ro', name: 'Romanian' },
  { code: 'ru', name: 'Russian' },
  { code: 'sr', name: 'Serbian' },
  { code: 'sk', name: 'Slovak' },
  { code: 'sl', name: 'Slovenian' },
  { code: 'es', name: 'Spanish' },
  { code: 'es-MX', name: 'Spanish (Mexico)' },
  { code: 'es-AR', name: 'Spanish (Argentina)' },
  { code: 'sv', name: 'Swedish' },
  { code: 'th', name: 'Thai' },
  { code: 'tr', name: 'Turkish' },
  { code: 'uk', name: 'Ukrainian' },
  { code: 'vi', name: 'Vietnamese' },
] as const

export const ALL_RATING_SOURCES = [
  { key: 'imdb', label: 'IMDb', color: '#b4910f' },
  { key: 'tmdb', label: 'TMDB', color: '#019b58' },
  { key: 'rt', label: 'Rotten Tomatoes (Critics)', color: '#b92308' },
  { key: 'rta', label: 'Rotten Tomatoes (Audience)', color: '#b92308' },
  { key: 'mc', label: 'Metacritic', color: '#4b9626' },
  { key: 'trakt', label: 'Trakt', color: '#af0f2d' },
  { key: 'lb', label: 'Letterboxd', color: '#009b58' },
  { key: 'mal', label: 'MyAnimeList', color: '#223c78' },
  { key: 'mdblist', label: 'MDBList', color: '#4284CA' },
  { key: 'ebert', label: 'Roger Ebert', color: '#e8590c' },
] as const

export const DEFAULT_RATINGS_ORDER = 'mal,imdb,lb,rt,mc,rta,tmdb,trakt,mdblist,ebert'

// Maps from the short enum codes used by the image query params / settings API
// to human-readable labels. Used to show what a server "default" resolves to in
// the free-key "Try it out" form. `d` (style/direction) means "auto" — resolved
// at render time from the badge position/direction.
export const BADGE_STYLE_LABELS: Record<string, string> = { h: 'Horizontal', v: 'Vertical', d: 'Auto' }
export const BADGE_DIRECTION_LABELS: Record<string, string> = { h: 'Horizontal', v: 'Vertical', d: 'Auto' }
export const LABEL_STYLE_LABELS: Record<string, string> = { t: 'Text', i: 'Icon', o: 'Official' }
export const BADGE_SIZE_LABELS: Record<string, string> = { xs: 'Extra Small', s: 'Small', m: 'Medium', l: 'Large', xl: 'Extra Large' }
export const BADGE_SHAPE_LABELS: Record<string, string> = { r: 'Rounded', p: 'Pill' }
export const BADGE_BACKGROUND_LABELS: Record<string, string> = { d: 'Default', k: 'Dark', t: 'Transparent', n: 'None' }
export const IMAGE_SOURCE_LABELS: Record<string, string> = { t: 'TMDB', f: 'Fanart.tv' }
// Quality overlay badge. `quality` tiers are caller-supplied per request and
// stackable (e.g. `4k,dv`); `quality_style` chooses between text chips and brand
// logos. `lang_icon` renders the title's main language as a flag or ISO code.
export const QUALITY_TIERS = [
  { key: '4k', label: '4K' },
  { key: '1080p', label: '1080p' },
  { key: '720p', label: '720p' },
  { key: 'hdr', label: 'HDR' },
  { key: 'dv', label: 'Dolby Vision' },
] as const
export const QUALITY_STYLE_LABELS: Record<string, string> = { text: 'Text', logo: 'Logo' }
export const LANG_ICON_LABELS: Record<string, string> = { off: 'Off', flag: 'Flag', text: 'Text' }
export const POSTER_FIT_LABELS: Record<string, string> = { native: 'Native', cover: 'Crop to 2:3', blur: 'Blur fill', pad: 'Letterbox' }
export const POSITION_LABELS: Record<string, string> = {
  bc: 'Bottom Center', tc: 'Top Center', l: 'Left', r: 'Right',
  tl: 'Top Left', tr: 'Top Right', bl: 'Bottom Left', br: 'Bottom Right',
}

export function parseRatingsOrder(order: string): string[] {
  const keys = order ? order.split(',').map(k => k.trim()).filter(Boolean) : []
  const allKeys = ALL_RATING_SOURCES.map(s => s.key)
  for (const k of allKeys) {
    if (!keys.includes(k)) keys.push(k)
  }
  return keys
}

/**
 * Parse the comma-separated `ratings_exclude` string into an array of source
 * keys. Unlike {@link parseRatingsOrder}, this does NOT append missing sources —
 * it keeps only the keys the user has explicitly chosen to exclude. Unknown keys
 * are dropped so a stale value can't poison the form.
 */
export function parseRatingsExclude(exclude: string): string[] {
  if (!exclude) return []
  const known = new Set<string>(ALL_RATING_SOURCES.map(s => s.key))
  return exclude.split(',').map(k => k.trim()).filter(k => known.has(k))
}
