<script lang="ts">
  // Saat ve tarih. Saniye sayacı yok: her dakikanın başında bir kez güncellenir.
  import { onMount } from 'svelte';
  import { hhmm } from '../../lib/today';

  let { big = false }: { big?: boolean } = $props();

  let now = $state(new Date());
  onMount(() => {
    let timer: ReturnType<typeof setTimeout>;
    const tick = () => {
      now = new Date();
      timer = setTimeout(tick, 60_000 - (Date.now() % 60_000) + 50);
    };
    tick();
    return () => clearTimeout(timer);
  });
  const date = $derived(
    `${now.getDate()} ${now.toLocaleDateString('tr-TR', { month: 'long' })} · ${now.toLocaleDateString('tr-TR', { weekday: 'long' })}`,
  );
</script>

<div class="clock" class:big>
  <div class="hm">{hhmm(now)}</div>
  <div class="dt">{date}</div>
</div>

<style>
  .clock {
    background: var(--surface);
    border: 1px solid var(--line);
    box-shadow: var(--card-depth);
    border-radius: 16px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 10px;
    text-align: center;
  }
  .hm {
    font-size: 40px;
    font-weight: 700;
    letter-spacing: -1px;
    line-height: 1;
    font-variant-numeric: tabular-nums;
  }
  .big .hm {
    font-size: 46px;
  }
  .dt {
    color: var(--text-3);
    font-size: var(--fs-12);
  }
</style>
