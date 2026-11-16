<script lang="ts">
  // Yuvarlak yüzde göstergesi (CPU, GPU, RAM). Değer değişince yeni yay opacity ile belirir (dasharray geçişi yok).
  interface Props {
    value: number | null;
    label: string;
    sub: string;
    color: string;
    size?: number;
  }
  let { value, label, sub, color, size = 74 }: Props = $props();

  const R = 15.9;
  const C = 2 * Math.PI * R;
  const pct = $derived(value === null ? 0 : Math.max(0, Math.min(100, value)));
</script>

<div class="gauge">
  <svg viewBox="0 0 40 40" style:width="{size}px" style:height="{size}px" aria-hidden="true">
    <circle cx="20" cy="20" r={R} class="track" />
    {#key Math.round(pct)}<circle cx="20" cy="20" r={R} class="arc" style:stroke={color} stroke-dasharray="{(C * pct) / 100} {C}" transform="rotate(-90 20 20)" />{/key}
    <text x="20" y="22.6" text-anchor="middle">{value === null ? '–' : `%${Math.round(pct)}`}</text>
  </svg>
  <span class="lab">{label}</span>
  <span class="sub">{sub}</span>
</div>

<style>
  .gauge {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
  }
  circle {
    fill: none;
    stroke-width: 3.4;
  }
  .track {
    stroke: var(--surface-3);
  }
  .arc {
    stroke-linecap: round;
    animation: arc-in var(--t-fast) var(--ease) both;
  }
  @keyframes arc-in {
    from {
      opacity: 0.35;
    }
  }
  text {
    font-size: var(--fs-xs);
    font-weight: 700;
    fill: var(--text);
  }
  .lab {
    font-weight: 650;
  }
  .sub {
    color: var(--text-3);
    font-size: var(--fs-11);
    white-space: nowrap;
  }
</style>
