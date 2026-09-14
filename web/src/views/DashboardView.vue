<script setup lang="ts">
import { ref } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import { adminApi } from '@/lib/api'
import RefreshButton from '@/components/RefreshButton.vue'
import ClearCacheButton from '@/components/ClearCacheButton.vue'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'

interface Stats {
  total_images: number
  total_api_keys: number
  mem_cache_entries: number
  id_cache_entries: number
  ratings_cache_entries: number
  image_mem_cache_mb: number
}

const { data: stats, isPending, isFetching, refetch } = useQuery<Stats>({
  queryKey: ['admin', 'stats'],
  queryFn: async () => {
    const res = await adminApi.getStats()
    if (!res.ok) throw new Error('Failed to fetch stats')
    return res.json()
  },
})

const cards = [
  { key: 'total_images', label: 'Total Images' },
  { key: 'total_api_keys', label: 'API Keys' },
  { key: 'mem_cache_entries', label: 'Memory Cache Entries' },
  { key: 'id_cache_entries', label: 'ID Cache Entries' },
  { key: 'ratings_cache_entries', label: 'Ratings Cache Entries' },
  { key: 'image_mem_cache_mb', label: 'Image Cache (MB)' },
] as const

const clearMessage = ref('')
function onCleared(message: string) {
  clearMessage.value = message
  refetch()
}
</script>

<template>
  <div class="space-y-4">
    <div class="flex items-center justify-end gap-2">
      <ClearCacheButton @cleared="onCleared" />
      <RefreshButton :fetching="isFetching" @refresh="refetch()" />
    </div>
    <p v-if="clearMessage" class="text-sm text-muted-foreground text-right">{{ clearMessage }}</p>
    <div class="grid gap-4 grid-cols-1 sm:grid-cols-2 lg:grid-cols-3">
    <Card v-for="card in cards" :key="card.key">
      <CardHeader class="pb-2">
        <CardTitle class="text-sm font-medium text-muted-foreground">{{ card.label }}</CardTitle>
      </CardHeader>
      <CardContent>
        <Skeleton v-if="isPending" class="h-8 w-20" />
        <p v-else class="text-2xl font-bold">{{ stats?.[card.key] ?? '—' }}</p>
      </CardContent>
    </Card>
    </div>
  </div>
</template>
