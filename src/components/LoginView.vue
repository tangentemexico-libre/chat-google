<script setup lang="ts">
import { onMounted, ref } from "vue";
import {
  credentialsStatus,
  getBrowserPreference,
  listBrowsers,
  loginGoogle,
  openDataDir,
  setBrowserPreference,
  type BrowserOption,
  type CredentialsStatus,
  type TokenInfo,
} from "../services/tauri";

const emit = defineEmits<{
  loggedIn: [TokenInfo];
}>();

const creds = ref<CredentialsStatus | null>(null);
const browsers = ref<BrowserOption[]>([]);
const browserId = ref("default");
const loading = ref(false);
const error = ref("");

onMounted(async () => {
  try {
    creds.value = await credentialsStatus();
    browsers.value = await listBrowsers();
    const pref = await getBrowserPreference();
    browserId.value = pref.browser_id || "default";
  } catch (e) {
    error.value = String(e);
  }
});

async function refreshCreds() {
  creds.value = await credentialsStatus();
}

async function openFolder() {
  error.value = "";
  try {
    await openDataDir();
    await refreshCreds();
  } catch (e) {
    error.value = String(e);
  }
}

async function onBrowserChange() {
  error.value = "";
  try {
    await setBrowserPreference(browserId.value);
  } catch (e) {
    error.value = String(e);
  }
}

async function login() {
  error.value = "";
  loading.value = true;
  try {
    await setBrowserPreference(browserId.value);
    const token = await loginGoogle();
    emit("loggedIn", token);
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}
</script>

<template>
  <div class="tc-login">
    <div class="tc-login-card">
      <h1>Tangente Chat</h1>
      <p class="lead">
        Cliente de escritorio para Google Chat. Tu Workspace, sin depender de una
        pestaña del navegador.
      </p>

      <p class="mb-1"><strong>1.</strong> Coloca el JSON OAuth (tipo Desktop) aquí:</p>
      <div class="tc-path">{{ creds?.path || "…" }}</div>
      <div class="d-flex gap-2 mb-3">
        <button class="tc-btn tc-btn-light" type="button" @click="openFolder">
          Abrir carpeta
        </button>
        <button class="tc-btn tc-btn-light" type="button" @click="refreshCreds">
          Verificar
        </button>
      </div>

      <p class="mb-2" :class="creds?.exists ? 'tc-alert ok' : 'tc-alert error'">
        {{
          creds?.exists
            ? "Credenciales encontradas."
            : "Aún no está el archivo google-oauth.client.json"
        }}
      </p>

      <p class="tc-status mb-3">
        Redirect URI a registrar en Google Cloud:
        <code>{{ creds?.redirect_uri }}</code>
      </p>

      <label class="tc-label" for="browser-select">Navegador para el login de Google</label>
      <select
        id="browser-select"
        v-model="browserId"
        class="tc-select mb-3"
        @change="onBrowserChange"
      >
        <option
          v-for="b in browsers"
          :key="b.id"
          :value="b.id"
          :disabled="!b.available"
        >
          {{ b.name }}{{ b.available ? "" : " (no instalado)" }}
        </option>
      </select>

      <button
        class="tc-btn tc-btn-primary w-100"
        type="button"
        :disabled="loading || !creds?.exists"
        @click="login"
      >
        {{ loading ? "Esperando Google…" : "Iniciar sesión con Google" }}
      </button>

      <div v-if="error" class="tc-alert error">{{ error }}</div>
    </div>
  </div>
</template>
