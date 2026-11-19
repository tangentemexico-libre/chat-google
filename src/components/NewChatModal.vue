<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import {
  listContacts,
  startDirectChat,
  type ContactItem,
  type SpaceItem,
} from "../services/tauri";

const emit = defineEmits<{
  close: [];
  started: [SpaceItem];
}>();

const contacts = ref<ContactItem[]>([]);
const query = ref("");
const email = ref("");
const loading = ref(false);
const starting = ref(false);
const error = ref("");
const contactsError = ref("");

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase();
  if (!q) return contacts.value;
  return contacts.value.filter(
    (c) =>
      c.name.toLowerCase().includes(q) || c.email.toLowerCase().includes(q),
  );
});

onMounted(async () => {
  loading.value = true;
  try {
    contacts.value = await listContacts();
  } catch (e) {
    contactsError.value = String(e);
  } finally {
    loading.value = false;
  }
});

async function startWithEmail(raw: string) {
  const value = raw.trim();
  if (!value) return;
  starting.value = true;
  error.value = "";
  try {
    const space = await startDirectChat(value);
    emit("started", space);
  } catch (e) {
    error.value = String(e);
  } finally {
    starting.value = false;
  }
}

function onSubmitEmail() {
  void startWithEmail(email.value);
}

function pickContact(c: ContactItem) {
  email.value = c.email;
  void startWithEmail(c.email);
}
</script>

<template>
  <div class="tc-modal-backdrop" @click.self="emit('close')">
    <div class="tc-modal" role="dialog" aria-label="Nuevo chat">
      <header class="tc-modal-header">
        <h3>Nuevo chat</h3>
        <button class="tc-btn tc-btn-light" type="button" @click="emit('close')">Cerrar</button>
      </header>

      <div class="tc-modal-body">
        <label class="tc-label">Correo completo</label>
        <form class="tc-email-row" @submit.prevent="onSubmitEmail">
          <input
            v-model="email"
            type="email"
            placeholder="alguien@empresa.com"
            autocomplete="off"
            required
          />
          <button class="tc-btn tc-btn-primary" type="submit" :disabled="starting || !email.trim()">
            {{ starting ? "…" : "Iniciar" }}
          </button>
        </form>

        <label class="tc-label mt-3">O elige un contacto</label>
        <input
          v-model="query"
          type="search"
          class="tc-modal-search"
          placeholder="Filtrar contactos…"
        />

        <div class="tc-contact-list">
          <div v-if="loading" class="tc-status py-2">Cargando contactos…</div>
          <div v-else-if="contactsError" class="tc-alert error" style="margin:0.5rem">
            {{ contactsError }}
          </div>
          <button
            v-for="c in filtered"
            :key="c.email"
            type="button"
            class="tc-contact"
            :disabled="starting"
            @click="pickContact(c)"
          >
            <span class="tc-contact-name">{{ c.name }}</span>
            <span class="tc-contact-email">{{ c.email }}</span>
          </button>
          <div v-if="!loading && !contactsError && !filtered.length" class="tc-status py-2">
            Sin contactos en lista. Usa el correo completo arriba.
          </div>
        </div>

        <div v-if="error" class="tc-alert error mt-2">{{ error }}</div>
      </div>
    </div>
  </div>
</template>
