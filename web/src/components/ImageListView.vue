<script setup lang="ts">
import { ref, computed, onUnmounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useQuery } from '@tanstack/vue-query'
import { Eye, Loader2, Download, Trash2 } from 'lucide-vue-next'
import { titleIdFromCacheValue } from '@/lib/utils'
import RefreshButton from '@/components/RefreshButton.vue'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'

interface ImageMeta {
  cache_key: string
  release_date: string | null
  created_at: number
  updated_at: number
}

interface ListResponse {
  items: ImageMeta[]
  total: number
  page: number
  page_size: number
}

const props = defineProps<{
  kind: 'poster' | 'logo' | 'backdrop' | 'episode' | 'season'
  listFn: (page: number, pageSize: number) => Promise<Response>
  imageFn: (key: string) => Promise<Response>
  fetchFn: (idType: string, idValue: string) => Promise<Response>
  deleteFn: (idType: string, idValue: string, scope: 'title' | 'variant') => Promise<Response>
  clearAllFn: () => Promise<Response>
}>()

const route = useRoute()
const router = useRouter()

const page = computed({
  get: () => {
    const p = Number(route.query.page)
    return p > 0 ? p : 1
  },
  set: (v: number) => {
    router.replace({ query: { ...route.query, page: v === 1 ? undefined : String(v) } })
  },
})
const pageSize = 50

const { data, isPending, isFetching, refetch } = useQuery<ListResponse>({
  queryKey: computed(() => ['admin', props.kind + 's', page.value]),
  queryFn: async () => {
    const res = await props.listFn(page.value, pageSize)
    if (!res.ok) throw new Error(`Failed to fetch ${props.kind}s`)
    return res.json()
  },
})

const previewOpen = ref(false)
const previewKey = ref('')
const previewUrl = ref<string | null>(null)
const previewLoading = ref(false)

async function openPreview(cacheKey: string) {
  previewKey.value = cacheKey
  previewOpen.value = true
  previewLoading.value = true
  previewUrl.value = null

  try {
    const res = await props.imageFn(cacheKey)
    if (res.ok) {
      const blob = await res.blob()
      previewUrl.value = URL.createObjectURL(blob)
    }
  } finally {
    previewLoading.value = false
  }
}

function closePreview() {
  previewOpen.value = false
  if (previewUrl.value) {
    URL.revokeObjectURL(previewUrl.value)
    previewUrl.value = null
  }
}

onUnmounted(() => {
  if (previewUrl.value) URL.revokeObjectURL(previewUrl.value)
})

const fetchModalOpen = ref(false)
const fetchIdType = ref('imdb')
const fetchIdValue = ref('')
const fetchLoading = ref(false)
const fetchError = ref('')

async function fetchImage() {
  const idValue = fetchIdValue.value.trim()
  if (!idValue) return

  fetchLoading.value = true
  fetchError.value = ''

  try {
    const res = await props.fetchFn(fetchIdType.value, idValue)
    if (!res.ok) {
      const text = await res.text()
      try { fetchError.value = JSON.parse(text).error || text } catch { fetchError.value = text || `Error ${res.status}` }
      return
    }
    const fetchedKey = `${fetchIdType.value}/${idValue}`
    // Use the image bytes from the fetch response directly for preview,
    // since the cache key used by imageFn may not match the full variant key.
    const blob = await res.blob()
    fetchIdValue.value = ''
    fetchModalOpen.value = false
    previewKey.value = fetchedKey
    previewOpen.value = true
    previewLoading.value = false
    previewUrl.value = URL.createObjectURL(blob)
    refetch()
  } catch (e) {
    fetchError.value = e instanceof Error ? e.message : 'Fetch failed'
  } finally {
    fetchLoading.value = false
  }
}

function openFetchModal() {
  fetchError.value = ''
  fetchModalOpen.value = true
}

// --- Purge: this exact variant (the row) or every variant of the title ---
const deleteOpen = ref(false)
const deleteTarget = ref<{ idType: string; cacheValue: string; titleId: string } | null>(null)
const deleteLoading = ref<'' | 'title' | 'variant'>('')
const deleteError = ref('')

function openDelete(cacheKey: string) {
  const { idType, idValue } = parseKey(cacheKey)
  deleteTarget.value = { idType, cacheValue: idValue, titleId: titleIdFromCacheValue(idValue) }
  deleteError.value = ''
  deleteOpen.value = true
}

async function confirmDelete(scope: 'title' | 'variant') {
  const target = deleteTarget.value
  if (!target || deleteLoading.value) return

  deleteLoading.value = scope
  deleteError.value = ''

  try {
    const idValue = scope === 'variant' ? target.cacheValue : target.titleId
    const res = await props.deleteFn(target.idType, idValue, scope)
    if (!res.ok) {
      const text = await res.text()
      try { deleteError.value = JSON.parse(text).error || text } catch { deleteError.value = text || `Error ${res.status}` }
      return
    }
    deleteOpen.value = false
    deleteTarget.value = null
    refetch()
  } catch (e) {
    deleteError.value = e instanceof Error ? e.message : 'Purge failed'
  } finally {
    deleteLoading.value = ''
  }
}

// --- Clear every cached image of this kind ---
const clearAllOpen = ref(false)
const clearAllLoading = ref(false)
const clearAllError = ref('')
const clearAllMessage = ref('')

function openClearAll() {
  clearAllError.value = ''
  clearAllMessage.value = ''
  clearAllOpen.value = true
}

async function confirmClearAll() {
  clearAllLoading.value = true
  clearAllError.value = ''

  try {
    const res = await props.clearAllFn()
    if (!res.ok) {
      const text = await res.text()
      try { clearAllError.value = JSON.parse(text).error || text } catch { clearAllError.value = text || `Error ${res.status}` }
      return
    }
    const body = await res.json().catch(() => ({}))
    const n = body.meta_deleted ?? 0
    clearAllMessage.value = `Cleared ${n} cached ${n === 1 ? kindLabel.value : kindLabelPlural.value}.`
    clearAllOpen.value = false
    page.value = 1
    refetch()
  } catch (e) {
    clearAllError.value = e instanceof Error ? e.message : 'Clear failed'
  } finally {
    clearAllLoading.value = false
  }
}

function parseKey(cacheKey: string) {
  const idx = cacheKey.indexOf('/')
  if (idx === -1) return { idType: cacheKey, idValue: '' }
  return { idType: cacheKey.slice(0, idx), idValue: cacheKey.slice(idx + 1) }
}

function formatDate(epoch: number) {
  return new Date(epoch * 1000).toLocaleDateString()
}

function relativeTime(epoch: number) {
  const diff = Date.now() / 1000 - epoch
  if (diff < 60) return 'just now'
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  return `${Math.floor(diff / 86400)}d ago`
}

const totalPages = computed(() => data.value ? Math.ceil(data.value.total / data.value.page_size) : 0)

const kindLabel = computed(() => props.kind)
const kindLabelPlural = computed(() => props.kind + 's')

function prevPage() {
  if (page.value > 1) page.value--
}

function nextPage() {
  if (page.value < totalPages.value) page.value++
}

const previewSizeClass = computed(() => {
  if (props.kind === 'backdrop') return 'max-w-2xl'
  if (props.kind === 'episode') return 'max-w-xl'
  return 'max-w-md'
})

const skeletonClass = computed(() => {
  if (props.kind === 'poster' || props.kind === 'season') return 'h-[400px] w-[270px] max-w-full rounded-md'
  if (props.kind === 'logo') return 'h-[200px] w-[400px] max-w-full rounded-md'
  return 'h-[270px] w-[480px] max-w-full rounded-md'
})
</script>

<template>
  <div class="space-y-4">
    <div class="flex justify-end gap-2">
      <Button variant="outline" size="sm" @click="openFetchModal">
        <Download class="size-4 mr-1" />
        Fetch
      </Button>
      <Button variant="outline" size="sm" class="text-destructive hover:text-destructive" @click="openClearAll">
        <Trash2 class="size-4 mr-1" />
        Clear {{ kindLabelPlural }}
      </Button>
      <RefreshButton :fetching="isFetching" @refresh="refetch()" />
    </div>
    <p v-if="clearAllMessage" class="text-sm text-muted-foreground text-right">{{ clearAllMessage }}</p>
    <div v-if="isPending" class="space-y-3">
      <Skeleton v-for="i in 5" :key="i" class="h-10 w-full" />
    </div>
    <template v-else-if="data">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead class="w-10"></TableHead>
            <TableHead>ID Type</TableHead>
            <TableHead>ID Value</TableHead>
            <TableHead>Release Date</TableHead>
            <TableHead>Last Updated</TableHead>
            <TableHead>Created</TableHead>
            <TableHead class="w-10 text-right"><span class="sr-only">Actions</span></TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          <TableRow v-if="data.items.length === 0">
            <TableCell colspan="7" class="text-center text-muted-foreground">No {{ kindLabelPlural }} cached yet.</TableCell>
          </TableRow>
          <TableRow v-for="item in data.items" :key="item.cache_key" class="cursor-pointer" @click="openPreview(item.cache_key)">
            <TableCell>
              <Eye class="size-4 text-muted-foreground" />
            </TableCell>
            <TableCell class="font-mono text-xs">{{ parseKey(item.cache_key).idType }}</TableCell>
            <TableCell class="font-mono text-xs">{{ parseKey(item.cache_key).idValue }}</TableCell>
            <TableCell>{{ item.release_date || '—' }}</TableCell>
            <TableCell>{{ relativeTime(item.updated_at) }}</TableCell>
            <TableCell>{{ formatDate(item.created_at) }}</TableCell>
            <TableCell class="text-right">
              <Button
                variant="ghost"
                size="icon"
                class="size-8 text-muted-foreground hover:text-destructive"
                :aria-label="`Purge ${kindLabel}`"
                :title="`Purge cached ${kindLabel}`"
                @click.stop="openDelete(item.cache_key)"
              >
                <Trash2 class="size-4" />
              </Button>
            </TableCell>
          </TableRow>
        </TableBody>
      </Table>
      <div class="flex items-center justify-between">
        <p class="text-sm text-muted-foreground">
          {{ data.total }} {{ data.total === 1 ? kindLabel : kindLabelPlural }} total
        </p>
        <div class="flex items-center gap-2">
          <Button variant="outline" size="sm" :disabled="page <= 1" @click="prevPage">Previous</Button>
          <span class="text-sm">Page {{ page }} of {{ totalPages }}</span>
          <Button variant="outline" size="sm" :disabled="page >= totalPages" @click="nextPage">Next</Button>
        </div>
      </div>
    </template>

    <Dialog :open="fetchModalOpen" @update:open="(v: boolean) => { if (!v) fetchModalOpen = false }">
      <DialogContent class="max-w-sm">
        <DialogHeader>
          <DialogTitle>Fetch {{ kindLabel.charAt(0).toUpperCase() + kindLabel.slice(1) }}</DialogTitle>
        </DialogHeader>
        <form class="space-y-4" @submit.prevent="fetchImage">
          <div class="space-y-2">
            <Label>ID Type</Label>
            <Select v-model="fetchIdType">
              <SelectTrigger data-testid="fetch-id-type-select">
                <SelectValue placeholder="Select ID type" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="imdb">IMDb</SelectItem>
                <SelectItem value="tmdb">TMDb</SelectItem>
                <SelectItem value="tvdb">TVDB</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-2">
            <Label>ID Value</Label>
            <Input v-model="fetchIdValue" placeholder="e.g. tt1234567" />
          </div>
          <p v-if="fetchError" class="text-sm text-destructive">{{ fetchError }}</p>
          <div class="flex justify-end">
            <Button type="submit" :disabled="fetchLoading || !fetchIdValue.trim()">
              <Loader2 v-if="fetchLoading" class="size-4 animate-spin mr-1" />
              Fetch
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>

    <Dialog :open="clearAllOpen" @update:open="(v: boolean) => { if (!v) clearAllOpen = false }">
      <DialogContent class="max-w-sm">
        <DialogHeader>
          <DialogTitle>Clear {{ kindLabelPlural }}</DialogTitle>
        </DialogHeader>
        <div class="space-y-4">
          <p class="text-sm text-muted-foreground">
            Remove <strong>all</strong> cached {{ kindLabelPlural }}<span v-if="data"> ({{ data.total }} title{{ data.total === 1 ? '' : 's' }})</span>?
            Other image types are unaffected. They regenerate on the next request.
          </p>
          <p v-if="clearAllError" class="text-sm text-destructive">{{ clearAllError }}</p>
          <div class="flex justify-end gap-2">
            <Button variant="outline" :disabled="clearAllLoading" @click="clearAllOpen = false">Cancel</Button>
            <Button variant="destructive" :disabled="clearAllLoading" @click="confirmClearAll">
              <Loader2 v-if="clearAllLoading" class="size-4 animate-spin mr-1" />
              Clear {{ kindLabelPlural }}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>

    <Dialog :open="deleteOpen" @update:open="(v: boolean) => { if (!v) { deleteOpen = false; deleteTarget = null } }">
      <DialogContent class="max-w-md">
        <DialogHeader>
          <DialogTitle>Purge {{ kindLabel }}</DialogTitle>
        </DialogHeader>
        <div class="space-y-4">
          <p class="text-sm text-muted-foreground">
            Remove just this one cached {{ kindLabel }} variant, or every variant of
            this title? Anything removed is regenerated on the next request.
          </p>
          <div class="space-y-1 rounded-md border bg-muted/40 p-2 text-xs font-mono break-all">
            <p><span class="text-muted-foreground">this variant: </span>{{ deleteTarget?.idType }}/{{ deleteTarget?.cacheValue }}</p>
            <p><span class="text-muted-foreground">entire title: </span>{{ deleteTarget?.idType }}/{{ deleteTarget?.titleId }}</p>
          </div>
          <p v-if="deleteError" class="text-sm text-destructive">{{ deleteError }}</p>
          <div class="flex flex-wrap justify-end gap-2">
            <Button variant="outline" :disabled="!!deleteLoading" @click="deleteOpen = false">Cancel</Button>
            <Button variant="outline" class="text-destructive hover:text-destructive" :disabled="!!deleteLoading" @click="confirmDelete('variant')">
              <Loader2 v-if="deleteLoading === 'variant'" class="size-4 animate-spin mr-1" />
              This variant
            </Button>
            <Button variant="destructive" :disabled="!!deleteLoading" @click="confirmDelete('title')">
              <Loader2 v-if="deleteLoading === 'title'" class="size-4 animate-spin mr-1" />
              Entire title
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>

    <Dialog :open="previewOpen" @update:open="(v: boolean) => { if (!v) closePreview() }">
      <DialogContent :class="previewSizeClass">
        <DialogHeader>
          <DialogTitle class="min-w-0 break-all pr-8 font-mono text-sm">{{ previewKey }}</DialogTitle>
        </DialogHeader>
        <div class="flex items-center justify-center min-h-[200px] min-w-0">
          <Skeleton v-if="previewLoading" :class="skeletonClass" />
          <img
            v-else-if="previewUrl"
            :src="previewUrl"
            :alt="previewKey"
            class="max-h-[70vh] max-w-full rounded-md object-contain"
          />
          <p v-else class="text-sm text-muted-foreground">Failed to load {{ kindLabel }}</p>
        </div>
      </DialogContent>
    </Dialog>
  </div>
</template>
