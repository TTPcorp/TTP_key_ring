<script lang="ts">
  interface Props {
    /** Estimated entropy of the password, in bits. */
    bits: number;
  }
  let { bits }: Props = $props();

  /** Full bar at 128 bits, the default generator target. */
  const FULL = 128;

  // Red → green as the entropy grows; blue with a shield from 128 bits.
  const levels = [
    { min: 0, label: "Muy baja" },
    { min: 40, label: "Baja" },
    { min: 60, label: "Media" },
    { min: 80, label: "Buena" },
    { min: 100, label: "Alta" },
    { min: FULL, label: "Muy alta" },
  ];

  const level = $derived.by(() => {
    let index = 0;
    levels.forEach((l, i) => {
      if (bits >= l.min) index = i;
    });
    return index;
  });
  const veryHigh = $derived(bits >= FULL);
  const label = $derived((levels[level]?.label ?? "Muy baja").toLowerCase());
</script>

<div class="strength level-{level}">
  <progress max={FULL} value={Math.min(bits, FULL)} aria-label="Seguridad de la contraseña"
  ></progress>
  <span class="strength-label">
    {#if veryHigh}
      <svg class="shield" viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
        <path
          fill="currentColor"
          d="M12 2 4 5v6c0 5 3.4 9.7 8 11 4.6-1.3 8-6 8-11V5l-8-3Zm-1.2 14.2-3.5-3.5 1.4-1.4 2.1 2.1 4.9-4.9 1.4 1.4-6.3 6.3Z"
        />
      </svg>
    {/if}
    Seguridad {label} · {Math.round(bits)} bits
  </span>
</div>
