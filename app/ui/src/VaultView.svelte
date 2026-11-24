<script lang="ts">
  import { onMount } from "svelte";
  import { listEntries, lockVault, errorMessage, type EntrySummary } from "./api";

  interface Props {
    onLocked: () => void;
  }
  let { onLocked }: Props = $props();

  let query = $state("");
  let entries = $state<EntrySummary[]>([]);
  let error = $state("");

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

<header class="toolbar">
  <input
    type="search"
    placeholder="Buscar por nombre, usuario o web…"
    aria-label="Buscar"
    bind:value={query}
    oninput={load}
  />
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
      <p class="muted">Pronto podrás añadir entradas desde aquí.</p>
    {/if}
  </section>
{:else}
  <ul class="entries">
    {#each entries as entry (entry.id)}
      <li class="card">
        <span class="name">{entry.name}</span>
        <span class="muted">{entry.username}</span>
        {#if entry.urls.length > 0}
          <span class="url">{entry.urls[0]}</span>
        {/if}
      </li>
    {/each}
  </ul>
{/if}
