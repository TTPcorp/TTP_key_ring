<script lang="ts">
  import { applyTheme, loadTheme, type Theme } from "./theme";

  let theme = $state<Theme>(loadTheme());

  const choices: { value: Theme; label: string }[] = [
    { value: "system", label: "Sistema" },
    { value: "light", label: "Claro" },
    { value: "dark", label: "Oscuro" },
  ];

  function choose(value: Theme) {
    theme = value;
    applyTheme(value);
  }
</script>

<div class="theme-switch" role="group" aria-label="Tema">
  {#each choices as choice (choice.value)}
    <button
      type="button"
      aria-pressed={theme === choice.value}
      onclick={() => choose(choice.value)}
    >
      {choice.label}
    </button>
  {/each}
</div>
