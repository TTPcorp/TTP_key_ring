<script lang="ts">
  import {
    createVault,
    unlockVault,
    errorMessage,
    estimateStrength,
    strengthWarningMessages,
    type StrengthEstimate,
  } from "./api";
  import StrengthMeter from "./StrengthMeter.svelte";

  interface Props {
    exists: boolean;
    onUnlocked: () => void;
  }
  let { exists, onUnlocked }: Props = $props();

  /** Minimum length required by the UI when creating a vault. */
  const MIN_MASTER_LENGTH = 12;

  let password = $state("");
  let confirmation = $state("");
  let acknowledged = $state(false);
  let busy = $state(false);
  let error = $state("");

  // Strength of the new master password, while typing (create only).
  let estimate = $state<StrengthEstimate | null>(null);
  let estimateTimer: ReturnType<typeof setTimeout> | undefined;
  let estimateSeq = 0;

  function scheduleEstimate() {
    if (exists) return;
    clearTimeout(estimateTimer);
    const seq = ++estimateSeq;
    if (password === "") {
      estimate = null;
      return;
    }
    estimateTimer = setTimeout(async () => {
      try {
        const result = await estimateStrength(password, { name: "", username: "", urls: [] });
        if (seq === estimateSeq) estimate = result;
      } catch {
        if (seq === estimateSeq) estimate = null;
      }
    }, 150);
  }

  const createProblem = $derived.by(() => {
    if (password.length < MIN_MASTER_LENGTH) {
      return `Usa al menos ${MIN_MASTER_LENGTH} caracteres.`;
    }
    if (password !== confirmation) {
      return "Las contraseñas no coinciden.";
    }
    if (!acknowledged) {
      return "Confirma que entiendes que no hay recuperación.";
    }
    return "";
  });

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    error = "";
    busy = true;
    try {
      if (exists) {
        await unlockVault(password);
      } else {
        if (createProblem) return;
        await createVault(password);
      }
      onUnlocked();
    } catch (err) {
      error = errorMessage(err);
    } finally {
      // Best effort: do not keep the password around in the page state.
      password = "";
      confirmation = "";
      clearTimeout(estimateTimer);
      estimateSeq++;
      estimate = null;
      busy = false;
    }
  }
</script>

<section class="card narrow">
  <h1>LLavero Digital</h1>

  {#if exists}
    <p class="muted">Introduce tu contraseña maestra para abrir el llavero.</p>
  {:else}
    <p class="muted">
      No hay ningún llavero todavía. Crea uno protegido con una contraseña maestra.
    </p>
  {/if}

  <form onsubmit={submit}>
    <label>
      Contraseña maestra
      <!-- svelte-ignore a11y_autofocus -->
      <input
        type="password"
        bind:value={password}
        autocomplete={exists ? "current-password" : "new-password"}
        oninput={scheduleEstimate}
        autofocus
        required
        disabled={busy}
      />
    </label>

    {#if !exists && estimate !== null && password !== ""}
      <StrengthMeter
        bits={estimate.bits}
        estimated
        warning={estimate.warning ? strengthWarningMessages[estimate.warning] : null}
      />
    {/if}

    {#if !exists}
      <label>
        Repite la contraseña
        <input
          type="password"
          bind:value={confirmation}
          autocomplete="new-password"
          required
          disabled={busy}
        />
      </label>

      <div class="warning">
        <strong>No hay forma de recuperar la contraseña maestra.</strong>
        Si la olvidas, o si pierdes el archivo del llavero, perderás todas las contraseñas guardadas.
        Haz copias de seguridad del archivo.
      </div>

      <label class="check">
        <input type="checkbox" bind:checked={acknowledged} disabled={busy} />
        Lo entiendo
      </label>

      {#if password || confirmation}
        <p class="hint">{createProblem || "Todo correcto."}</p>
      {/if}
    {/if}

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}

    <button type="submit" disabled={busy || (!exists && createProblem !== "")}>
      {#if busy}
        {exists ? "Abriendo…" : "Creando…"}
      {:else}
        {exists ? "Abrir llavero" : "Crear llavero"}
      {/if}
    </button>
  </form>
</section>
