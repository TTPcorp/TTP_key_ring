<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import {
    addEntry,
    checkAutotypeSequence,
    checkUrl,
    DEFAULT_AUTOTYPE,
    sequenceProblemMessage,
    deleteEntry,
    errorMessage,
    estimateStrength,
    generatePassword,
    getEntry,
    revealSecret,
    strengthWarningMessages,
    updateEntry,
    webWarningMessages,
    type GeneratorOptions,
    type SequenceProblem,
    type WindowForm,
    type StrengthEstimate,
    type WebAddressInfo,
  } from "./api";
  import StrengthMeter from "./StrengthMeter.svelte";

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
    targetBits: 128,
    length: 20,
    lowercase: true,
    uppercase: true,
    digits: true,
    symbols: true,
    excludeAmbiguous: false,
  });
  let entropyBits = $state<number | null>(null);
  let generatedLength = $state(0);

  // Live estimate for typed (or revealed) passwords. Generated ones use
  // their exact entropy instead.
  let estimate = $state<StrengthEstimate | null>(null);
  let estimateTimer: ReturnType<typeof setTimeout> | undefined;
  let estimateSeq = 0;

  function scheduleEstimate() {
    clearTimeout(estimateTimer);
    const seq = ++estimateSeq;
    if (password === "") {
      estimate = null;
      return;
    }
    estimateTimer = setTimeout(async () => {
      try {
        const result = await estimateStrength(password, {
          name,
          username,
          urls: urlsText.split("\n"),
        });
        // Ignore answers to older keystrokes.
        if (seq === estimateSeq) estimate = result;
      } catch {
        if (seq === estimateSeq) estimate = null;
      }
    }, 150);
  }

  /** Generator presets: entropy targets (powers of two) or a fixed length. */
  const targets: { value: number | null; label: string }[] = [
    { value: 64, label: "64 bits" },
    { value: 128, label: "128 bits (recomendado)" },
    { value: 256, label: "256 bits" },
    { value: null, label: "Longitud fija" },
  ];

  // Offline check of each web address, refreshed while typing.
  type UrlCheck = { line: string; info?: WebAddressInfo; error?: string };
  let urlChecks = $state<UrlCheck[]>([]);
  let urlTimer: ReturnType<typeof setTimeout> | undefined;

  function scheduleUrlCheck() {
    clearTimeout(urlTimer);
    urlTimer = setTimeout(runUrlCheck, 300);
  }

  async function runUrlCheck() {
    const lines = urlsText
      .split("\n")
      .map((l) => l.trim())
      .filter((l) => l !== "");
    urlChecks = await Promise.all(
      lines.map(async (line): Promise<UrlCheck> => {
        try {
          return { line, info: await checkUrl(line) };
        } catch (err) {
          return { line, error: errorMessage(err) };
        }
      }),
    );
  }

  // Auto-type: associated program windows and the typing sequence.
  let windows = $state<WindowForm[]>([]);
  let autotype = $state("");
  let sequenceProblem = $state<SequenceProblem | null>(null);
  let sequenceTimer: ReturnType<typeof setTimeout> | undefined;

  function scheduleSequenceCheck() {
    clearTimeout(sequenceTimer);
    sequenceTimer = setTimeout(async () => {
      const text = autotype;
      try {
        const problem = await checkAutotypeSequence(text);
        if (text === autotype) sequenceProblem = problem;
      } catch {
        sequenceProblem = null;
      }
    }, 200);
  }

  function addWindow() {
    windows = [...windows, { title: "", process: "" }];
  }

  function removeWindow(index: number) {
    windows = windows.filter((_, i) => i !== index);
  }

  let masterPassword = $state("");
  let deleteError = $state("");
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
      windows = entry.windows;
      autotype = entry.autotype;
      runUrlCheck();
      if (autotype !== "") scheduleSequenceCheck();
    } catch (err) {
      error = errorMessage(err);
    } finally {
      loading = false;
    }
  });

  // Best effort: do not leave secrets in the page state after closing.
  onDestroy(() => {
    clearTimeout(urlTimer);
    clearTimeout(estimateTimer);
    clearTimeout(sequenceTimer);
    password = "";
    notes = "";
    masterPassword = "";
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
        scheduleEstimate();
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
      generatedLength = generated.length;
      clearTimeout(estimateTimer);
      estimateSeq++;
      estimate = null;
      passwordTouched = true;
      showPassword = true;
    } catch (err) {
      error = errorMessage(err);
    }
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
      windows,
      autotype,
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

  async function remove(event: SubmitEvent) {
    event.preventDefault();
    if (id === null || busy || masterPassword === "") return;
    busy = true;
    deleteError = "";
    try {
      await deleteEntry(id, masterPassword);
      onClose(true);
    } catch (err) {
      deleteError = errorMessage(err);
    } finally {
      masterPassword = "";
      busy = false;
    }
  }

  function cancelDelete() {
    confirmingDelete = false;
    masterPassword = "";
    deleteError = "";
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
            oninput={() => {
              passwordTouched = true;
              entropyBits = null;
              scheduleEstimate();
            }}
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
          <StrengthMeter bits={entropyBits} />
          <p class="hint">Contraseña generada de {generatedLength} caracteres.</p>
        {:else if estimate !== null && password !== ""}
          <StrengthMeter
            bits={estimate.bits}
            estimated
            warning={estimate.warning ? strengthWarningMessages[estimate.warning] : null}
          />
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
              Seguridad
              <select bind:value={options.targetBits}>
                {#each targets as target (target.label)}
                  <option value={target.value}>{target.label}</option>
                {/each}
              </select>
            </label>
            {#if options.targetBits === null}
              <label class="inline">
                Longitud
                <input type="number" min="8" max="256" bind:value={options.length} />
              </label>
            {:else}
              <p class="hint">La longitud se calcula para llegar a esa seguridad.</p>
            {/if}
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
          oninput={scheduleUrlCheck}
          rows="2"
          spellcheck="false"
          placeholder="https://ejemplo.com"
          disabled={busy}
        ></textarea>
      </label>
      {#if urlChecks.length > 0}
        <ul class="url-checks" aria-live="polite">
          {#each urlChecks as check, i (i)}
            <li>
              {#if check.error}
                <span class="bad">✗ {check.line}: {check.error}</span>
              {:else if check.info && check.info.warnings.length > 0}
                <span class="warn">⚠ {check.info.host}</span>
                {#each check.info.warnings as warning (warning)}
                  <span class="hint">{webWarningMessages[warning]}</span>
                {/each}
              {:else if check.info}
                <span class="good">✓ {check.info.host}</span>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}

      <fieldset class="autotype">
        <legend>Auto-type en programas</legend>
        <p class="hint">
          Al pulsar Ctrl+Alt+A en la ventana de un programa asociado, se escribe la secuencia. En el
          título, * vale por cualquier texto (por ejemplo, «Banco*»).
        </p>
        {#each windows as win, i (i)}
          <div class="window-row">
            <label>
              Título de la ventana
              <input
                type="text"
                bind:value={win.title}
                placeholder="Inicio de sesión*"
                spellcheck="false"
                disabled={busy}
              />
            </label>
            <label>
              Programa
              <input
                type="text"
                bind:value={win.process}
                placeholder="programa.exe"
                spellcheck="false"
                disabled={busy}
              />
            </label>
            <button
              type="button"
              class="secondary small"
              aria-label="Quitar ventana {i + 1}"
              onclick={() => removeWindow(i)}
              disabled={busy}
            >
              Quitar
            </button>
            {#if win.process.trim() === "" && win.title.trim() !== ""}
              <p class="hint wide">
                Solo por título: cualquier programa o web puede imitarlo, así que se pedirá confirmación
                antes de escribir. Indica también el programa para que se escriba directamente.
              </p>
            {/if}
          </div>
        {/each}
        <div>
          <button type="button" class="secondary small" onclick={addWindow} disabled={busy}>
            Añadir ventana
          </button>
        </div>
        <label>
          Secuencia (vacía = por defecto)
          <input
            type="text"
            bind:value={autotype}
            oninput={scheduleSequenceCheck}
            placeholder={DEFAULT_AUTOTYPE}
            spellcheck="false"
            disabled={busy}
          />
        </label>
        {#if sequenceProblem && autotype.trim() !== ""}
          <p class="bad">✗ {sequenceProblemMessage(sequenceProblem)}</p>
        {:else}
          <p class="hint">
            Marcadores: {"{USERNAME}"}, {"{PASSWORD}"}, {"{TAB}"}, {"{ENTER}"} y {"{DELAY n}"} (pausa
            de n ms). El resto se escribe tal cual.
          </p>
        {/if}
      </fieldset>

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
          {#if !confirmingDelete}
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

    {#if confirmingDelete}
      <form class="delete-confirm" onsubmit={remove}>
        <p>
          <strong>¿Borrar «{name}» definitivamente?</strong> No se puede deshacer. Escribe tu contraseña
          maestra para confirmarlo.
        </p>
        <label>
          Contraseña maestra
          <!-- svelte-ignore a11y_autofocus -->
          <input
            type="password"
            bind:value={masterPassword}
            autocomplete="current-password"
            autofocus
            required
            disabled={busy}
          />
        </label>
        {#if deleteError}
          <p class="error" role="alert">{deleteError}</p>
        {/if}
        <div class="actions">
          <button type="submit" class="danger" disabled={busy || masterPassword === ""}>
            {busy ? "Comprobando…" : "Borrar definitivamente"}
          </button>
          <button type="button" class="secondary" onclick={cancelDelete} disabled={busy}>
            Cancelar
          </button>
        </div>
      </form>
    {/if}
  {/if}
</section>
