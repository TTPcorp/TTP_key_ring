<script lang="ts">
  import { onMount } from "svelte";
  import { onDestroy } from "svelte";
  import {
    copyToClipboard,
    listEntries,
    lockVault,
    errorMessage,
    type CopyField,
    type EntrySummary,
  } from "./api";
  import EntryForm from "./EntryForm.svelte";

  interface Props {
    onLocked: () => void;
  }
  let { onLocked }: Props = $props();

  let query = $state("");
  let entries = $state<EntrySummary[]>([]);
  let error = $state("");
  /** Open form: `{ id: null }` for a new entry, `null` when showing the list. */
  let form = $state<{ id: string | null } | null>(null);

  // Notice shown after copying, with a countdown until it is cleared.
  let notice = $state<{ text: string; seconds: number } | null>(null);
  let countdown: ReturnType<typeof setInterval> | undefined;

  async function copy(entry: EntrySummary, field: CopyField) {
    try {
      const seconds = await copyToClipboard(entry.id, field);
      const what = field === "password" ? "Contraseña copiada" : "Usuario copiado";
      notice = { text: `${what} («${entry.name}»).`, seconds };
      clearInterval(countdown);
      countdown = setInterval(() => {
        if (notice === null || notice.seconds <= 1) {
          clearInterval(countdown);
          notice = null;
        } else {
          notice = { ...notice, seconds: notice.seconds - 1 };
        }
      }, 1000);
      error = "";
    } catch (err) {
      error = errorMessage(err);
    }
  }

  onDestroy(() => clearInterval(countdown));

  function closeForm(changed: boolean) {
    form = null;
    if (changed) load();
  }

  async function load() {
    try {
      entries = await listEntries(query);
      error = "";
    } catch (err) {
      error = errorMessage(err);
    }
  }

  async function lock() {
    try {
      await lockVault();
    } finally {
      onLocked();
    }
  }

  onMount(load);
</script>

{#if form}
  {#key form.id}
    <EntryForm id={form.id} onClose={closeForm} />
  {/key}
{:else}
  <header class="toolbar">
    <input
      type="search"
      placeholder="Buscar por nombre, usuario o web…"
      aria-label="Buscar"
      bind:value={query}
      oninput={load}
    />
    <button onclick={() => (form = { id: null })}>Nueva entrada</button>
    <button class="secondary" onclick={lock}>Bloquear</button>
  </header>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}

  {#if notice}
    <p class="notice" role="status">
      {notice.text} Se borrará del portapapeles en {notice.seconds} s.
    </p>
  {/if}

  {#if entries.length === 0}
    <section class="card empty">
      {#if query}
        <p>No hay entradas que coincidan con «{query}».</p>
      {:else}
        <p>El llavero está vacío.</p>
        <p class="muted">Pulsa «Nueva entrada» para guardar tu primera contraseña.</p>
      {/if}
    </section>
  {:else}
    <ul class="entries">
      {#each entries as entry (entry.id)}
        <li class="card entry-row">
          <button class="entry" onclick={() => (form = { id: entry.id })}>
            <span class="name">{entry.name}</span>
            <span class="muted">{entry.username}</span>
            {#if entry.urls.length > 0}
              <span class="url">{entry.urls[0]}</span>
            {/if}
          </button>
          <div class="copy-actions">
            {#if entry.username}
              <button
                class="secondary small"
                aria-label="Copiar usuario de {entry.name}"
                onclick={() => copy(entry, "username")}
              >
                Copiar usuario
              </button>
            {/if}
            <button
              class="secondary small"
              aria-label="Copiar contraseña de {entry.name}"
              onclick={() => copy(entry, "password")}
            >
              Copiar contraseña
            </button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
{/if}
