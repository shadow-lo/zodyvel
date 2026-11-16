<script lang="ts">
  // Görünüm: tema (vurgu rengi), ada zemini, düzen (Zody / Saat), hareketleri azalt, sekme çubuğundaki
  // uygulama kısayolları. Seçim hemen adaya da uygulanır.
  import { data } from '../../lib/store.svelte';
  import { addApp, appHue, appIcon, appLetter, isWebApp, moveApp, pickApp, removeApp } from '../../lib/apps.svelte';
  import { MAX_APPS } from '../../lib/types';
  import { SURFACES, THEMES, surfaceById } from '../../lib/theme';
  import Icon from '../Icon.svelte';
  import { update } from '../state.svelte';

  const current = $derived(data.settings.appearance);
  const island = $derived(surfaceById(current.surface).island);
  const apps = $derived(data.settings.apps ?? []);

  // Site ya da elle yol ekleme. Uygulama seçici adadaki "+" ile aynı.
  let url = $state('');
  let label = $state('');
  let appError = $state<string | null>(null);
  async function addUrl(event: SubmitEvent) {
    event.preventDefault();
    const target = /^[a-z]+:\/\//i.test(url.trim()) || /^[a-z]:\\/i.test(url.trim()) ? url.trim() : `https://${url.trim()}`;
    appError = await addApp(target, label);
    if (!appError) url = label = '';
  }
  async function pick() {
    appError = await pickApp();
  }

</script>

<header class="page-head">
  <h1>Görünüm</h1>
  <p>Tema adayı, Zody'nin rengini ve bu pencereyi birlikte boyar. Durum renkleri (onay turuncu, bitti yeşil, hata kırmızı) her temada aynı kalır.</p>
</header>

<section class="group">
  <h3>Tema</h3>
  <div class="themes" role="radiogroup" aria-label="Tema">
    {#each THEMES as t (t.id)}
      <button
        class="theme"
        class:on={current.theme === t.id}
        role="radio"
        aria-checked={current.theme === t.id}
        style:--a={t.accent}
        style:--a2={t.accent2}
        style:--isl={island}
        onclick={() => update(s => (s.appearance.theme = t.id))}
      >
        <span class="mini" aria-hidden="true">
          <span class="shape">
            <span class="icons"><i class="sel"></i><i></i><i></i><i></i></span>
            <span class="card"><b class="dot"></b><em></em><u></u><s></s></span>
          </span>
        </span>
        <span class="name"><b>{t.name}</b><small>{t.note}</small></span>
      </button>
    {/each}
  </div>
</section>

<section class="group">
  <h3>Ada</h3>
  <div class="row">
    <span class="ic"><Icon name="island" /></span>
    <span class="tx"><b>Ada rengi</b><small>Çentiğin ve kartların zemini</small></span>
    <span class="seg">
      {#each SURFACES as s (s.id)}
        <button class:on={current.surface === s.id} onclick={() => update(n => (n.appearance.surface = s.id))}>{s.name}</button>
      {/each}
    </span>
  </div>
  <div class="row">
    <span class="ic"><Icon name="clock" /></span>
    <span class="tx"><b>Düzen</b><small>Zody: üzerine gelince mini AI. Saat: büyük saat ve kartlar; sekmeler ikisinde aynı</small></span>
    <span class="seg">
      <button class:on={current.layout !== 'clock'} onclick={() => update(n => (n.appearance.layout = 'zody'))}>Zody</button>
      <button class:on={current.layout === 'clock'} onclick={() => update(n => (n.appearance.layout = 'clock'))}>Saat</button>
    </span>
  </div>
  <label class="row">
    <span class="ic"><Icon name="motion" /></span>
    <span class="tx"><b>Hareketleri azalt</b><small>Geçiş animasyonlarını ve dönen plağı kapatır</small></span>
    <input class="switch" type="checkbox" role="switch" checked={data.settings.island.reduceMotion} onchange={e => update(s => (s.island.reduceMotion = e.currentTarget.checked))} />
  </label>
</section>

<section class="group">
  <h3>Uygulama kısayolları</h3>
  <p class="note">Sekme çubuğunun sağında en çok {MAX_APPS} uygulama ya da site. Adadaki "+" uygulama seçer; site buradan eklenir.</p>
  {#each apps as app, i (app.id)}
    {@const icon = appIcon(app)}
    <div class="row">
      <span class="app" style:--hue={appHue(app)} class:img={!!icon}>
        {#if icon}<img src={icon} alt="" />{:else if isWebApp(app)}🌐{:else}{appLetter(app)}{/if}
      </span>
      <span class="tx"><b>{app.name}</b><small class="target">{app.target}</small></span>
      <button class="btn" disabled={i === 0} aria-label="Yukarı taşı" onclick={() => moveApp(app.id, -1)}>↑</button>
      <button class="btn" disabled={i === apps.length - 1} aria-label="Aşağı taşı" onclick={() => moveApp(app.id, 1)}>↓</button>
      <button class="btn" onclick={() => removeApp(app.id)}>Kaldır</button>
    </div>
  {/each}
  {#if apps.length < MAX_APPS}
    <form class="row add" onsubmit={addUrl}>
      <input type="text" placeholder="Site adresi (ör. mail.google.com)" bind:value={url} aria-label="Site adresi" />
      <input type="text" class="short" placeholder="Ad (isteğe bağlı)" bind:value={label} aria-label="Kısayol adı" />
      <button class="btn primary" type="submit" disabled={!url.trim()}>Ekle</button>
      <button class="btn" type="button" onclick={pick}>Uygulama seç…</button>
    </form>
  {/if}
  {#if appError}<p class="note err" role="status">{appError}</p>{/if}
</section>

<style>
  .note {
    margin: 0;
    padding: 0 16px 10px;
    color: var(--text-3);
    font-size: var(--fs-11);
  }
  .note.err {
    color: var(--red);
    padding-top: 8px;
  }
  .app {
    width: 30px;
    height: 30px;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: 9px;
    color: #fff;
    font-weight: 650;
    background: hsl(var(--hue) 55% 42%);
  }
  .app.img {
    background: var(--surface-2);
  }
  .app img {
    width: 22px;
    height: 22px;
  }
  .target {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 360px;
  }
  .add input {
    flex: 1;
    min-width: 0;
    padding: 7px 10px;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--surface-2);
    color: var(--text);
    font: inherit;
  }
  .add input.short {
    flex: 0 0 150px;
  }
  /* Mini ada önizlemesi: tüm kutucuklarda aynı kalıp (şerit altından sarkan ada). Her kutucuk kendi
     renklerini (--a vurgu, --isl ada zemini) gösterir. */
  .mini {
    height: 84px;
    border-radius: 9px;
    /* Masaüstü zemini: tema renginden hafif tonlu, ada ondan ayrışsın. */
    background: color-mix(in srgb, var(--a) 9%, #20232e);
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.05);
    display: flex;
    justify-content: center;
    overflow: hidden;
  }
  .shape {
    width: 74%;
    align-self: flex-start;
    border-radius: 0 0 13px 13px;
    background: var(--isl);
    padding: 7px 8px 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    box-shadow: 0 6px 14px rgb(0 0 0 / 0.35);
  }
  .icons {
    display: flex;
    justify-content: center;
    gap: 3px;
  }
  .icons i {
    width: 15px;
    height: 11px;
    border-radius: 4px;
    background: rgb(255 255 255 / 0.1);
  }
  .icons i.sel {
    background: color-mix(in srgb, var(--a) 28%, rgb(255 255 255 / 0.1));
  }
  .card {
    position: relative;
    border-radius: 7px;
    background: #14161d;
    padding: 8px 26px 8px 8px;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .card em {
    width: 62%;
    height: 5px;
    border-radius: 3px;
    background: #eceef3;
  }
  .card u {
    width: 86%;
    height: 4px;
    border-radius: 2px;
    background: #6e7383;
  }
  .card s {
    position: absolute;
    right: 7px;
    top: 50%;
    width: 13px;
    height: 13px;
    margin-top: -6.5px;
    border-radius: 50%;
    background: var(--a);
  }
  /* Tema kutucuğunda Zody (vurgu + halka) kartın başında, çizgiler tema renginde. */
  .theme .card {
    padding-left: 30px;
  }
  .theme .card em {
    background: var(--a);
    width: 70%;
  }
  .theme .card .dot {
    position: absolute;
    left: 7px;
    top: 50%;
    width: 16px;
    height: 16px;
    margin-top: -8px;
    border-radius: 50%;
    background: radial-gradient(circle at 35% 30%, color-mix(in srgb, var(--a) 40%, #fff), var(--a) 65%);
    box-shadow:
      0 0 8px color-mix(in srgb, var(--a) 55%, transparent),
      0 0 0 1.5px #14161d,
      0 0 0 2.5px var(--a2);
  }
  .theme .card s {
    background: var(--a2);
  }
  .themes {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
    padding: 12px 16px 16px;
  }
  .theme {
    border-radius: 12px;
    border: 1px solid var(--line);
    background: var(--surface-2);
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    text-align: left;
    transition:
      border-color 140ms,
      transform 140ms var(--spring);
  }
  .theme:hover {
    border-color: var(--line-strong);
    transform: translateY(-1px);
  }
  .theme.on {
    border-color: var(--a);
    box-shadow: inset 0 0 0 1px var(--a);
  }
  .name b {
    display: block;
    font-size: var(--fs-12-5);
  }
  .name small {
    color: var(--text-3);
    font-size: var(--fs-11);
  }
</style>
