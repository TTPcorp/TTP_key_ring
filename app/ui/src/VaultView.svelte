<script lang="ts">
  import { onMount } from "svelte";
  import { listEntries, lockVault, errorMessage, type EntrySummary } from "./api";
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
        <li>
          <button class="card entry" onclick={() => (form = { id: entry.id })}>
            <span class="name">{entry.name}</span>
            <span class="muted">{entry.username}</span>
            {#if entry.urls.length > 0}
              <span class="url">{entry.urls[0]}</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
{/if}
