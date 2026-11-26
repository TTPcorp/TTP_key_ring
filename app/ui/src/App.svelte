<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import {
    autotypeNotice,
    userActivity,
    vaultStatus,
    errorMessage,
    type AutoTypeNotice,
    type VaultStatus,
  } from "./api";
  import AutoTypePicker from "./AutoTypePicker.svelte";
  import Unlock from "./Unlock.svelte";
  import VaultView from "./VaultView.svelte";
  import ThemeSwitch from "./ThemeSwitch.svelte";

  let status = $state<VaultStatus | null>(null);
  let loadError = $state("");

  async function refresh() {
    try {
      status = await vaultStatus();
      loadError = "";
    } catch (err) {
      loadError = errorMessage(err);
    }
  }

  // The backend locks the vault by itself (inactivity, Win+L). While it is
  // open, check every 2 s so the screen switches to "locked" at once.
  // Polling does not count as activity.
  const poll = setInterval(() => {
    if (status?.unlocked) refresh();
  }, 2000);

  // Tell the backend about user input, at most every 15 s.
  const ACTIVITY_EVERY_MS = 15_000;
  let lastActivity = 0;
  function onActivity() {
    const now = Date.now();
    if (status?.unlocked && now - lastActivity > ACTIVITY_EVERY_MS) {
      lastActivity = now;
      userActivity().catch(() => {});
    }
  }

  // Auto-type: the backend shows the window when a press of the shortcut
  // needs the user (locked vault, picking an entry, an error). Check on
  // focus and every second; this does not count as activity either.
  let autotype = $state<AutoTypeNotice | null>(null);
  async function checkAutotype() {
    try {
      const notice = await autotypeNotice();
      if (notice) {
        autotype = notice;
        await refresh();
      }
    } catch {
      // Not critical: the next check will retry.
    }
  }
  const autotypePoll = setInterval(checkAutotype, 1000);

  function closePicker(error?: unknown) {
    autotype = error === undefined ? null : { kind: "failed", error };
  }

  function autotypeMessage(notice: AutoTypeNotice): string {
    switch (notice.kind) {
      case "hotkeyUnavailable":
        return `Otro programa ya usa el atajo ${notice.hotkey}: el auto-type no funcionará mientras lo use.`;
      case "locked":
        return `Para usar el auto-type, desbloquea el llavero, vuelve a la ventana del programa y pulsa ${notice.hotkey} otra vez.`;
      case "noMatch":
        return `Ninguna entrada está asociada a la ventana «${notice.window || "sin título"}».`;
      case "failed":
        return errorMessage(notice.error);
      case "choose":
        return "";
    }
  }

  onMount(() => {
    refresh();
    checkAutotype();
  });
  onDestroy(() => {
    clearInterval(poll);
    clearInterval(autotypePoll);
  });
</script>

<svelte:window
  onfocus={checkAutotype}
  onkeydown={onActivity}
  onpointerdown={onActivity}
  onpointermove={onActivity}
  onwheel={onActivity}
/>

<main>
  <ThemeSwitch />
  {#if autotype && autotype.kind !== "choose"}
    <p class={autotype.kind === "failed" ? "error" : "notice"} role="status">
      {autotypeMessage(autotype)}
      <button class="link" onclick={() => (autotype = null)}>Cerrar</button>
    </p>
  {/if}
  {#if autotype?.kind === "choose" && status?.unlocked}
    <AutoTypePicker
      requestId={autotype.requestId}
      window={autotype.window}
      entries={autotype.entries}
      onClose={closePicker}
    />
  {/if}
  {#if loadError}
    <p class="error" role="alert">{loadError}</p>
  {:else if status === null}
    <p class="muted">Cargando…</p>
  {:else if status.unlocked}
    <VaultView onLocked={refresh} />
  {:else}
    <Unlock exists={status.exists} lockReason={status.lockReason} onUnlocked={refresh} />
  {/if}
</main>
