<script setup lang="ts">
import { ref } from 'vue'
import { Trash2, Loader2 } from 'lucide-vue-next'
import { adminApi } from '@/lib/api'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'

withDefaults(defineProps<{ size?: 'sm' | 'default' }>(), { size: 'sm' })
const emit = defineEmits<{ cleared: [message: string] }>()

const open = ref(false)
const loading = ref(false)
const error = ref('')

async function confirmClear() {
  loading.value = true
  error.value = ''

  try {
    const res = await adminApi.purgeAll()
    if (!res.ok) {
      const text = await res.text()
      try { error.value = JSON.parse(text).error || text } catch { error.value = text || `Error ${res.status}` }
      return
    }
    const body = await res.json()
    const message = body.external_cache_only
      ? 'Cache metadata cleared. Images are served from an external CDN and could not be purged from here.'
      : `Cache cleared — removed ${body.meta_deleted} cached image${body.meta_deleted === 1 ? '' : 's'}.`
    open.value = false
    emit('cleared', message)
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Clear failed'
  } finally {
    loading.value = false
  }
}

function openDialog() {
  error.value = ''
  open.value = true
}
</script>

<template>
  <Button variant="outline" :size="size" class="text-destructive hover:text-destructive" @click="openDialog">
    <Trash2 class="size-4 mr-1" />
    Clear cache
  </Button>

  <Dialog :open="open" @update:open="(v: boolean) => { if (!v) open = false }">
    <DialogContent class="max-w-sm">
      <DialogHeader>
        <DialogTitle>Clear cache</DialogTitle>
      </DialogHeader>
      <div class="space-y-4">
        <p class="text-sm text-muted-foreground">
          This removes every cached poster, logo, backdrop, and episode — rendered
          images, raw downloads, and in-memory caches. Images are regenerated on the
          next request, so the first load of each title will be slower.
        </p>
        <p v-if="error" class="text-sm text-destructive">{{ error }}</p>
        <div class="flex justify-end gap-2">
          <Button variant="outline" :disabled="loading" @click="open = false">Cancel</Button>
          <Button variant="destructive" :disabled="loading" @click="confirmClear">
            <Loader2 v-if="loading" class="size-4 animate-spin mr-1" />
            Clear cache
          </Button>
        </div>
      </div>
    </DialogContent>
  </Dialog>
</template>
