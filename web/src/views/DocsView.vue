<script setup lang="ts">
import { ref, onMounted } from "vue";
import { ApiReference } from "@scalar/api-reference";
import "@scalar/api-reference/style.css";
import { ArrowLeft } from "lucide-vue-next";

const topbar = ref<HTMLElement>();
const headerHeight = ref("0px");

onMounted(() => {
  if (topbar.value) {
    headerHeight.value = `${topbar.value.offsetHeight}px`;
  }
});
</script>

<template>
  <div class="docs-page" :style="{ '--topbar-height': headerHeight }">
    <header ref="topbar" class="docs-topbar">
      <router-link to="/" class="docs-back-link">
        <ArrowLeft class="docs-back-icon" />
        <span class="docs-title">OpenPosterDB</span>
      </router-link>
      <span class="docs-subtitle">API Reference</span>
    </header>
    <ApiReference :configuration="{
      url: '/api/openapi.json',
      hideClientButton: true,
      showDeveloperTools: 'never',
      mcp: { disabled: true },
      agent: { disabled: true },
      forceDarkModeState: 'light',
      hideDarkModeToggle: true,
      defaultOpenAllTags: true,
    }" />
  </div>
</template>

<style scoped>
.docs-page :deep(.scalar-app) {
  --scalar-custom-header-height: var(--topbar-height);
}

.docs-topbar {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.625rem 1rem;
  border-bottom: 1px solid #e5e5e5;
  background: #fff;
  position: sticky;
  top: 0;
  z-index: 100;
}

.docs-back-link {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  text-decoration: none;
  color: #0a0a0a;
  font-weight: 600;
  font-size: 0.875rem;
}

.docs-back-link:hover {
  color: #525252;
}

.docs-back-icon {
  width: 1rem;
  height: 1rem;
}

.docs-subtitle {
  color: #a3a3a3;
  font-size: 0.875rem;
}
</style>
