<script lang="ts">
  // Bilgisayar: minide üç gösterge (tıkla → ayrıntı); ayrıntıda göstergeler, en çok bellek
  // kullanan uygulamalar ve Görev Yöneticisi. Ölçüm yalnız bu bölüm ekrandayken yapılır.
  // Pili olan bilgisayarda (Özellikler → Pil açıksa) dördüncü gösterge pil; Windows değişince bildirir.
  // Yeni görünümde ayrıntı daha kısa (levels.ts), liste altı uygulama, Görev Yöneticisi başlıkta bir bağlantı.
  import { duration, gb, mb } from '../../lib/format';
  import { data, openTaskManager } from '../../lib/store.svelte';
  import type { Battery, SystemLoad } from '../../lib/types';
  import Gauge from '../parts/Gauge.svelte';

  interface Props {
    system: SystemLoad | null;
    battery: Battery | null;
    detail: boolean;
    onopen: () => void;
  }
  let { system, battery, detail, onopen }: Props = $props();

  const batteryColor = $derived(!battery ? '' : battery.plugged || battery.percent > 20 ? 'var(--green)' : battery.percent > 10 ? 'var(--amber)' : 'var(--red)');
  const batterySub = $derived(
    !battery
      ? ''
      : battery.plugged
        ? battery.charging
          ? 'şarj oluyor'
          : 'prizde'
        : battery.minutesLeft !== null
          ? duration(battery.minutesLeft * 60_000)
          : battery.saver
            ? 'tasarruf açık'
            : 'pilde',
  );

  const ram = $derived(system && system.ramTotalGb > 0 ? (system.ramUsedGb / system.ramTotalGb) * 100 : null);
  const top = $derived(system?.topApps[0]?.ramMb ?? 1);
  const size = $derived(detail ? (battery ? 92 : 104) : battery ? 66 : 74);
  // Rust en çok altı uygulama gönderir; Klasik eskisi gibi beş gösterir.
  const apps = $derived(system?.topApps.slice(0, 6) ?? []);
</script>

<div class="body" class:detail>
  {#if !system}
    <p class="none">Ölçülüyor…</p>
  {:else}
    <button class="card gauges" style:--n={battery ? 4 : 3} onclick={detail ? openTaskManager : onopen} title={detail ? "Görev Yöneticisi'ni aç" : 'Ayrıntıyı aç'}>
      <Gauge value={system.cpu} label="CPU" sub="işlemci" color="var(--blue)" {size} />
      <Gauge value={system.gpu} label="GPU" sub="en yüksek motor" color="var(--accent)" {size} />
      <Gauge value={ram} label="RAM" sub="{gb(system.ramUsedGb)} / {gb(system.ramTotalGb)}" color="var(--amber)" {size} />
      {#if battery}
        <Gauge value={battery.percent} label="Pil" sub={batterySub} color={batteryColor} {size} />
      {/if}
    </button>
    {#if detail}
      <section class="card apps compact">
        <h4 title="Özel çalışma kümesi">En çok bellek<button class="tm-link" onclick={openTaskManager}>Görev Yöneticisi ↗</button></h4>
        {#each apps as app (app.name)}
          <div class="app">
            <i>{app.name.slice(0, 1).toUpperCase()}</i>
            <span class="name">{app.name}</span>
            <small>{mb(app.ramMb)}</small>
            <span class="bar"><b style:transform="scaleX({app.ramMb / top})"></b></span>
          </div>
        {/each}
      </section>
    {/if}
  {/if}
</div>

<style>
  .body {
    height: calc(100% - 36px);
    padding: 0 10px 10px;
    display: flex;
    gap: 8px;
  }
  .card {
    background: var(--surface);
    border: 1px solid var(--line);
    box-shadow: var(--card-depth);
    background-image: var(--card-sheen);
    border-radius: 16px;
  }
  .gauges {
    flex: 1;
    display: grid;
    grid-template-columns: repeat(var(--n, 3), 1fr);
    align-items: center;
    transition:
      background var(--t-fast),
      border-color var(--t-fast);
  }
  .gauges:hover {
    background: var(--surface-2);
    border-color: var(--line-strong);
  }
  .none {
    margin: auto;
    color: var(--text-3);
  }
  .apps {
    width: 250px;
    flex: none;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 9px;
  }
  h4 {
    margin: 0;
    font-size: var(--fs-12);
    color: var(--text-2);
    font-weight: 600;
    display: flex;
  }
  .app {
    display: grid;
    grid-template-columns: 22px 1fr auto;
    gap: 3px 8px;
    align-items: center;
    font-size: var(--fs-12);
  }
  .app i {
    grid-row: span 2;
    width: 22px;
    height: 22px;
    border-radius: 7px;
    background: var(--surface-3);
    display: grid;
    place-items: center;
    font-style: normal;
    font-size: var(--fs-11);
    font-weight: 700;
    color: var(--text-2);
  }
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .app small {
    color: var(--text-3);
  }
  .bar {
    grid-column: 2 / 4;
    height: 4px;
    border-radius: 2px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .bar b {
    display: block;
    height: 100%;
    background: var(--accent);
    transform-origin: left;
    transition: transform var(--t-slow) var(--ease);
  }
  /* Yeni görünüm: dar ve sık liste, başlıkta küçük Görev Yöneticisi bağlantısı. */
  .apps.compact {
    width: 236px;
    padding: 10px 12px;
    gap: 5px;
  }
  .compact h4 {
    align-items: center;
    color: var(--text);
    font-size: var(--fs-12-5);
    font-weight: 650;
  }
  .tm-link {
    margin-left: auto;
    color: var(--accent-soft);
    font-size: var(--fs-11);
    font-weight: 500;
  }
  .tm-link:hover {
    text-decoration: underline;
  }
  .compact .app {
    grid-template-columns: 16px 1fr auto;
    gap: 2px 7px;
  }
  .compact .app i {
    width: 16px;
    height: 16px;
    border-radius: 5px;
    font-size: var(--fs-10-5);
  }
  .compact .app small {
    font-size: var(--fs-11);
  }
  .compact .bar {
    height: 3px;
  }
</style>
