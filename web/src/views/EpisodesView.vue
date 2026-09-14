<script setup lang="ts">
import { adminApi } from '@/lib/api'
import ImageListView from '@/components/ImageListView.vue'
</script>

<template>
  <ImageListView
    kind="episode"
    :list-fn="adminApi.getEpisodes"
    :image-fn="adminApi.getEpisodeImage"
    :fetch-fn="adminApi.fetchEpisode"
    :delete-fn="adminApi.purgeEpisode"
    :clear-all-fn="adminApi.clearEpisodes"
  />
</template>
