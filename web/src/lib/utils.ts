import type { ClassValue } from "clsx"
import { clsx } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

/**
 * Extract the logical title id from a rendered cache value.
 *
 * A cache value is `{idValue}{variant}{suffix}`, where `variant` (when present)
 * starts with `_` and `suffix` always starts with `@`. Legitimate title ids
 * never contain `_` or `@`, so the title id is everything up to the first such
 * delimiter — e.g. `tt123_t_de@imc` and `movie-99@i` yield `tt123` / `movie-99`.
 */
export function titleIdFromCacheValue(cacheValue: string): string {
  const match = cacheValue.match(/^[^_@]*/)
  return match && match[0] ? match[0] : cacheValue
}
