<script setup lang="ts">
import { onMounted, ref } from "vue";
import ChatShell from "./components/ChatShell.vue";
import LoginView from "./components/LoginView.vue";
import { authStatus, type TokenInfo } from "./services/tauri";

const session = ref<TokenInfo | null>(null);
const booting = ref(true);
const bootError = ref("");

onMounted(async () => {
  try {
    session.value = await authStatus();
  } catch (e) {
    bootError.value = String(e);
  } finally {
    booting.value = false;
  }
});
</script>

<template>
  <div class="tc-app">
    <div v-if="booting" class="tc-login">
      <div class="tc-login-card">
        <h1>Tangente Chat</h1>
        <p class="lead">Abriendo…</p>
      </div>
    </div>

    <LoginView v-else-if="!session" @logged-in="session = $event" />
    <ChatShell
      v-else
      :key="session.email || 'session'"
      :session="session"
      @logged-out="session = null"
      @session-changed="session = $event"
    />

    <div v-if="bootError" class="px-3 py-2">
      <div class="tc-alert error">{{ bootError }}</div>
    </div>
  </div>
</template>
