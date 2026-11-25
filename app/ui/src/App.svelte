<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { userActivity, vaultStatus, errorMessage, type VaultStatus } from "./api";
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

  onMount(refresh);
  onDestroy(() => clearInterval(poll));
</script>

<svelte:window
  onkeydown={onActivity}
  onpointerdown={onActivity}
  onpointermove={onActivity}
  onwheel={onActivity}
/>

<main>
  <ThemeSwitch />
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
