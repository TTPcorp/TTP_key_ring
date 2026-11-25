<script lang="ts">
  import { onMount } from "svelte";
  import { vaultStatus, errorMessage, type VaultStatus } from "./api";
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

  onMount(refresh);
</script>

<main>
  <ThemeSwitch />
  {#if loadError}
    <p class="error" role="alert">{loadError}</p>
  {:else if status === null}
    <p class="muted">Cargando…</p>
  {:else if status.unlocked}
    <VaultView onLocked={refresh} />
  {:else}
    <Unlock exists={status.exists} onUnlocked={refresh} />
  {/if}
</main>
