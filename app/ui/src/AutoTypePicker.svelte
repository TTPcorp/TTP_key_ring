<script lang="ts">
  import { autotypeCancel, autotypeChoose, type AutoTypeCandidate } from "./api";

  interface Props {
    requestId: string;
    window: string;
    entries: AutoTypeCandidate[];
    /** Called when the picker closes; `error` is set if typing failed. */
    onClose: (error?: unknown) => void;
  }
  let { requestId, window, entries, onClose }: Props = $props();

  let busy = $state(false);
  let buttons = $state<HTMLButtonElement[]>([]);

  // Keyboard users can pick the first entry with Enter right away.
  $effect(() => {
    buttons[0]?.focus();
  });

  async function choose(entry: AutoTypeCandidate) {
    busy = true;
    try {
      // The app minimizes itself and types into the program's window.
      await autotypeChoose(requestId, entry.id);
      onClose();
    } catch (err) {
      onClose(err);
    }
  }

  async function cancel() {
    await autotypeCancel(requestId).catch(() => {});
    onClose();
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === "Escape" && !busy) cancel();
  }
</script>

<svelte:window {onkeydown} />

<div class="overlay">
  <div class="card autotype-picker" role="dialog" aria-modal="true" aria-labelledby="autotype-title">
    <h2 id="autotype-title">Auto-type</h2>
    <p>
      {entries.length === 1 ? "¿Escribir esta entrada" : "¿Qué entrada quieres escribir"} en la ventana
      «{window || "sin título"}»?
    </p>
    {#if entries.length === 1}
      <p class="hint">
        La entrada solo coincide por el título de la ventana, que cualquier programa o web puede
        imitar. Comprueba que es el programa correcto.
      </p>
    {/if}
    <ul class="entries">
      {#each entries as entry, i (entry.id)}
        <li>
          <button bind:this={buttons[i]} class="entry" disabled={busy} onclick={() => choose(entry)}>
            <span class="name">{entry.name}</span>
            <span class="muted">{entry.username}</span>
          </button>
        </li>
      {/each}
    </ul>
    <div class="actions">
      <button class="secondary" disabled={busy} onclick={cancel}>Cancelar</button>
    </div>
  </div>
</div>
