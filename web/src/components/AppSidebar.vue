<script setup lang="ts">
import { useRoute, useRouter } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import { LayoutDashboard, Image, KeyRound, Settings, LogOut } from 'lucide-vue-next'
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarRail,
  useSidebar,
} from '@/components/ui/sidebar'
import { Button } from '@/components/ui/button'

const route = useRoute()
const router = useRouter()
const auth = useAuthStore()
const { state, isMobile, setOpenMobile } = useSidebar()

const items = [
  { title: 'Dashboard', icon: LayoutDashboard, to: '/' },
  { title: 'Posters', icon: Image, to: '/posters' },
  { title: 'API Keys', icon: KeyRound, to: '/keys' },
  { title: 'Settings', icon: Settings, to: '/settings' },
]

function isActive(path: string) {
  return route.path === path
}

function onNavigate() {
  if (isMobile.value) setOpenMobile(false)
}

function handleLogout() {
  if (isMobile.value) setOpenMobile(false)
  auth.logout()
  router.push('/login')
}
</script>

<template>
  <Sidebar variant="inset" collapsible="icon">
    <SidebarHeader>
      <router-link to="/" class="flex items-center justify-center py-1 group-data-[state=expanded]/sidebar-wrapper:justify-start hover:opacity-80 transition-opacity" @click="onNavigate">
        <span class="font-bold text-lg whitespace-nowrap">{{ state === 'collapsed' ? 'OPDB' : 'OpenPosterDB' }}</span>
      </router-link>
    </SidebarHeader>
    <SidebarContent>
      <SidebarGroup>
        <SidebarMenu>
          <SidebarMenuItem v-for="item in items" :key="item.title">
            <SidebarMenuButton
              as-child
              :is-active="isActive(item.to)"
              :tooltip="item.title"
            >
              <router-link :to="item.to" @click="onNavigate">
                <component :is="item.icon" />
                <span>{{ item.title }}</span>
              </router-link>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarGroup>
    </SidebarContent>
    <SidebarFooter>
      <SidebarMenu>
        <SidebarMenuItem>
          <SidebarMenuButton tooltip="Sign out" @click="handleLogout">
            <LogOut />
            <span>Sign out</span>
          </SidebarMenuButton>
        </SidebarMenuItem>
      </SidebarMenu>
    </SidebarFooter>
    <SidebarRail />
  </Sidebar>
</template>
