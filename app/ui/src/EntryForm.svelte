<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import {
    addEntry,
    deleteEntry,
    errorMessage,
    generatePassword,
    getEntry,
    revealSecret,
    updateEntry,
    type GeneratorOptions,
  } from "./api";

  interface Props {
    /** Entry to edit, or `null` to create a new one. */
    id: string | null;
    /** Called when the form closes; `changed` is true if the vault changed. */
    onClose: (changed: boolean) => void;
  }
  let { id, onClose }: Props = $props();

  const editing = $derived(id !== null);

  let loading = $state(false);
  let name = $state("");
  let username = $state("");
  let urlsText = $state("");

  // Secrets are only loaded on an explicit "Mostrar". Until the user
  // reveals or types into a field it stays "untouched" and the backend
  // keeps the stored value.
  let password = $state("");
  let passwordTouched = $state(false);
  let showPassword = $state(false);
  let hasPassword = $state(false);

  let notes = $state("");
  let notesTouched = $state(false);
  let hasNotes = $state(false);

  let generatorOpen = $state(false);
  let options = $state<GeneratorOptions>({
    length: 20,
    lowercase: true,
    uppercase: true,
    digits: true,
    symbols: true,
    excludeAmbiguous: false,
  });
  let entropyBits = $state<number | null>(null);

  let busy = $state(false);
  let error = $state("");
  let confirmingDelete = $state(false);

  onMount(async () => {
    if (id === null) return;
    loading = true;
    try {
      const entry = await getEntry(id);
      name = entry.name;
      username = entry.username;
      urlsText = entry.urls.join("\n");
      hasPassword = entry.hasPassword;
      hasNotes = entry.hasNotes;
    } catch (err) {
      error = errorMessage(err);
    } finally {
      loading = false;
    }
  });

  // Best effort: do not leave secrets in the page state after closing.
  onDestroy(() => {
    password = "";
    notes = "";
  });

  /** True while the stored notes exist but have not been revealed. */
  const notesHidden = $derived(editing && hasNotes && !notesTouched);

  async function togglePassword() {
    if (showPassword) {
      showPassword = false;
      return;
    }
    if (id !== null && hasPassword && !passwordTouched) {
      try {
        password = await revealSecret(id, "password");
        passwordTouched = true;
      } catch (err) {
        error = errorMessage(err);
        return;
      }
    }
    showPassword = true;
  }

  async function showNotes() {
    if (id === null) return;
    try {
      notes = await revealSecret(id, "notes");
      notesTouched = true;
    } catch (err) {
      error = errorMessage(err);
    }
  }

  async function generate() {
    error = "";
    try {
      const generated = await generatePassword(options);
      password = generated.password;
      entropyBits = generated.entropyBits;
      passwordTouched = true;
      showPassword = true;
    } catch (err) {
      error = errorMessage(err);
    }
  }

  function strengthLabel(bits: number): string {
    if (bits >= 100) return "muy fuerte";
    if (bits >= 75) return "fuerte";
    if (bits >= 50) return "aceptable";
    return "débil";
  }

  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    busy = true;
    error = "";
    const form = {
      name,
      username,
      urls: urlsText.split("\n"),
      password: !editing || passwordTouched ? password : null,
      notes: !editing || notesTouched ? notes : null,
    };
    try {
      if (id === null) {
        await addEntry(form);
      } else {
        await updateEntry(id, form);
      }
      onClose(true);
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }

  async function remove() {
    if (id === null || busy) return;
    busy = true;
    error = "";
    try {
      await deleteEntry(id);
      onClose(true);
    } catch (err) {
      error = errorMessage(err);
      confirmingDelete = false;
    } finally {
      busy = false;
    }
  }
</script>

<section class="card">
  <h1>{editing ? "Editar entrada" : "Nueva entrada"}</h1>

  {#if loading}
    <p class="muted">Cargando…</p>
  {:else}
    <form onsubmit={save}>
      <label>
        Nombre
        <!-- svelte-ignore a11y_autofocus -->
        <input type="text" bind:value={name} required autofocus disabled={busy} />
      </label>

      <label>
        Usuario o correo
        <input type="text" bind:value={username} autocomplete="off" disabled={busy} />
      </label>

      <div class="field">
        <label for="entry-password">Contraseña</label>
        <div class="row">
          <input
            id="entry-password"
            type={showPassword ? "text" : "password"}
            bind:value={password}
            oninput={() => (passwordTouched = true)}
            placeholder={editing && hasPassword && !passwordTouched
              ? "•••••••• (sin cambios)"
              : ""}
            autocomplete="new-password"
            spellcheck="false"
            disabled={busy}
          />
          <button type="button" class="secondary" onclick={togglePassword} disabled={busy}>
            {showPassword ? "Ocultar" : "Mostrar"}
          </button>
          <button type="button" class="secondary" onclick={generate} disabled={busy}>
            Generar
          </button>
        </div>
        {#if entropyBits !== null && passwordTouched}
          <p class="hint">
            Contraseña generada: unos {Math.round(entropyBits)} bits de entropía ({strengthLabel(
              entropyBits,
            )}).
          </p>
        {/if}
        <button
          type="button"
          class="link"
          aria-expanded={generatorOpen}
          onclick={() => (generatorOpen = !generatorOpen)}
        >
          {generatorOpen ? "Ocultar opciones del generador" : "Opciones del generador"}
        </button>
        {#if generatorOpen}
          <fieldset class="generator">
            <legend>Generador</legend>
            <label class="inline">
              Longitud
              <input type="number" min="8" max="256" bind:value={options.length} />
            </label>
            <label class="check">
              <input type="checkbox" bind:checked={options.lowercase} /> Minúsculas (a-z)
            </label>
            <label class="check">
              <input type="checkbox" bind:checked={options.uppercase} /> Mayúsculas (A-Z)
            </label>
            <label class="check">
              <input type="checkbox" bind:checked={options.digits} /> Números (0-9)
            </label>
            <label class="check">
              <input type="checkbox" bind:checked={options.symbols} /> Símbolos (!#$%…)
            </label>
            <label class="check">
              <input type="checkbox" bind:checked={options.excludeAmbiguous} />
              Evitar caracteres parecidos (I l 1 O 0)
            </label>
          </fieldset>
        {/if}
      </div>

      <label>
        Direcciones web (una por línea)
        <textarea
          bind:value={urlsText}
          rows="2"
          spellcheck="false"
          placeholder="https://ejemplo.com"
          disabled={busy}
        ></textarea>
      </label>

      <div class="field">
        <label for="entry-notes">Notas</label>
        {#if notesHidden}
          <div class="row">
            <p class="hint grow">Esta entrada tiene notas ocultas.</p>
            <button type="button" class="secondary" onclick={showNotes} disabled={busy}>
              Mostrar notas
            </button>
          </div>
        {:else}
          <textarea
            id="entry-notes"
            bind:value={notes}
            oninput={() => (notesTouched = true)}
            rows="3"
            disabled={busy}
          ></textarea>
        {/if}
      </div>

      {#if error}
        <p class="error" role="alert">{error}</p>
      {/if}

      <div class="actions">
        <button type="submit" disabled={busy}>{busy ? "Guardando…" : "Guardar"}</button>
        <button type="button" class="secondary" onclick={() => onClose(false)} disabled={busy}>
          Cancelar
        </button>
        {#if editing}
          <span class="grow"></span>
          {#if confirmingDelete}
            <span class="hint">¿Borrar definitivamente?</span>
            <button type="button" class="danger" onclick={remove} disabled={busy}>
              Sí, borrar
            </button>
            <button
              type="button"
              class="secondary"
              onclick={() => (confirmingDelete = false)}
              disabled={busy}
            >
              No
            </button>
          {:else}
            <button
              type="button"
              class="danger-outline"
              onclick={() => (confirmingDelete = true)}
              disabled={busy}
            >
              Borrar
            </button>
          {/if}
        {/if}
      </div>
    </form>
  {/if}
</section>
