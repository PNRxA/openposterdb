<script setup lang="ts">
import { ref, computed, onUnmounted } from 'vue'
import { useAuthStore } from '@/stores/auth'
import { FREE_API_KEY } from '@/lib/constants'
import { Button } from '@/components/ui/button'
import { Loader2 } from 'lucide-vue-next'

const auth = useAuthStore()

const idType = ref<'imdb' | 'tmdb' | 'tvdb'>('imdb')
const imageType = ref<'poster' | 'logo' | 'backdrop'>('poster')
const idValue = ref('tt0013442')
const fetchError = ref('')
const fetchLoading = ref(false)
const resultUrl = ref('')
const resultImageType = ref<'poster' | 'logo' | 'backdrop'>('poster')

const apiBase = import.meta.env.VITE_API_URL || ''

onUnmounted(() => {
  if (resultUrl.value) URL.revokeObjectURL(resultUrl.value)
})

const idPlaceholder = computed(() => {
  if (idType.value === 'imdb') return 'tt0013442'
  if (idType.value === 'tmdb') return 'movie-872585'
  return '253573'
})

const curlExample = computed(() => {
  const id = idValue.value.trim() || idPlaceholder.value
  const ext = imageType.value === 'logo' ? 'png' : 'jpg'
  return `curl -o ${imageType.value}.${ext} "${window.location.origin}/${FREE_API_KEY}/${idType.value}/${imageType.value}-default/${id}.${ext}"`
})

const resultClass = computed(() => {
  if (resultImageType.value === 'logo') return 'max-w-[400px]'
  if (resultImageType.value === 'backdrop') return 'max-w-[500px] rounded-lg shadow-lg'
  return 'max-w-[200px] rounded-lg shadow-lg'
})

async function handleFetch() {
  const id = idValue.value.trim()
  if (!id) return

  fetchError.value = ''
  fetchLoading.value = true

  const prevUrl = resultUrl.value

  const ext = imageType.value === 'logo' ? 'png' : 'jpg'
  const url = `${apiBase}/${FREE_API_KEY}/${idType.value}/${imageType.value}-default/${id}.${ext}`

  try {
    const res = await fetch(url)
    if (!res.ok) throw new Error(res.status === 404 ? 'Not found — check the ID and try again' : `Server error (${res.status})`)
    const blob = await res.blob()
    resultImageType.value = imageType.value
    resultUrl.value = URL.createObjectURL(blob)
    if (prevUrl) URL.revokeObjectURL(prevUrl)
  } catch (e) {
    fetchError.value = e instanceof Error && e.message ? e.message : 'Failed to fetch — check the ID and try again'
    if (prevUrl) URL.revokeObjectURL(prevUrl)
    resultUrl.value = ''
  } finally {
    fetchLoading.value = false
  }
}
</script>

<template>
  <div v-if="auth.freeApiKeyEnabled" class="rounded-lg border border-blue-500/30 bg-blue-500/5 p-4 space-y-3">
    <p class="text-sm font-medium">Free API Key Available</p>
    <p class="text-sm text-muted-foreground">
      Use the following key for poster serving (read-only, global defaults):
    </p>
    <code class="block text-sm font-mono bg-muted px-3 py-2 rounded select-all">{{ FREE_API_KEY }}</code>
    <div class="pt-2 space-y-3">
      <p class="text-sm font-medium">Try it out</p>
      <form class="flex flex-col sm:flex-row gap-2" @submit.prevent="handleFetch">
        <select
          id="free-id-type"
          v-model="idType"
          aria-label="ID type"
          class="rounded-md border border-input bg-background px-3 py-2 text-sm"
        >
          <option value="imdb">IMDb</option>
          <option value="tmdb">TMDb</option>
          <option value="tvdb">TVDB</option>
        </select>
        <select
          id="free-image-type"
          v-model="imageType"
          aria-label="Image type"
          class="rounded-md border border-input bg-background px-3 py-2 text-sm"
        >
          <option value="poster">Poster</option>
          <option value="logo">Logo</option>
          <option value="backdrop">Backdrop</option>
        </select>
        <input
          v-model="idValue"
          type="text"
          :placeholder="idPlaceholder"
          required
          class="flex-1 min-w-0 rounded-md border border-input bg-background px-3 py-2 text-sm font-mono"
        />
        <Button type="submit" size="sm" :disabled="fetchLoading">
          <Loader2 v-if="fetchLoading" class="h-4 w-4 animate-spin" />
          <span v-else>Fetch</span>
        </Button>
      </form>
      <code class="block text-xs font-mono bg-muted px-3 py-2 rounded text-muted-foreground break-all select-all">{{ curlExample }}</code>
      <p v-if="fetchError" class="text-sm text-destructive">{{ fetchError }}</p>
      <div v-if="resultUrl" class="flex justify-center pt-2 overflow-hidden">
        <img
          :src="resultUrl"
          alt="Fetched result"
          :class="['w-full', resultClass]"
        />
      </div>
    </div>
  </div>
</template>
