<script lang="ts">
  // Ada: ne zaman ve hangi bölümle açıldığı.
  import { onMount } from 'svelte';
  import { call, inTauri } from '../../lib/ipc';
  import { DEMO_SCREENS, missingScreen, SIZE } from '../../lib/screen';
  import { data } from '../../lib/store.svelte';
  import type { Screen, ScreenMode } from '../../lib/types';
  import Icon from '../Icon.svelte';
  import { update } from '../state.svelte';

  // Ekran listesi sayfa açılınca bir kez okunur; "Belirli ekran" seçilince tazelenir (ekran takılmış olabilir).
  let screens = $state<Screen[]>([]);
  async function loadScreens() {
    screens = (await call<Screen[]>('island_screens')) ?? (inTauri ? [] : DEMO_SCREENS);
  }
  onMount(() => void loadScreens());

  const MODES: { mode: ScreenMode; label: string }[] = [
    { mode: 'primary', label: 'Birincil' },
    { mode: 'fixed', label: 'Belirli ekran' },
    { mode: 'cursor', label: 'İmlecin olduğu' },
  ];
  function pickMode(mode: ScreenMode) {
    void update(s => {
      s.island.screen = mode;
      // İlk kez belirli ekran seçilince, kullanıcı listeden seçene kadar birincil ekran yazılır.
      if (mode === 'fixed' && !s.island.screenId) s.island.screenId = screens.find(x => x.primary)?.id ?? screens[0]?.id ?? '';
    });
    if (mode === 'fixed') void loadScreens();
  }
  const island = $derived(data.settings.island);
  const missing = $derived(island.screen === 'fixed' && missingScreen(screens, island.screenId));
</script>

<header class="page-head">
  <h1>Ada</h1>
  <p>Ekranın üst ortasındaki ince şerit. Üzerine gelince küçük görünüm, ortasına tıklayınca ayrıntı açılır; fare gidince yeniden şerit olur.</p>
</header>

<section class="group">
  <div class="row">
    <span class="ic"><Icon name="island" /></span>
    <span class="tx"><b>Açılma gecikmesi</b><small>Fare üst kenarda bu kadar bekleyince açılır. Sekmelere giderken yanlışlıkla açılıyorsa artır.</small></span>
    <input
      class="range"
      type="range"
      min="60"
      max="600"
      step="20"
      value={data.settings.island.hoverDelayMs}
      onchange={e => update(s => (s.island.hoverDelayMs = Number(e.currentTarget.value)))}
    />
    <output>{data.settings.island.hoverDelayMs} ms</output>
  </div>
  <div class="row">
    <span class="ic"><Icon name="motion" /></span>
    <span class="tx"
      ><b>Açılış</b><small
        >Üzerine gelince: Zody çıkar, kısa süre sonra küçük görünüm açılır. Tıklayınca: üzerine gelince yalnız Zody çıkar; tıkla küçük görünüm
        (AI, mini oynatıcı…), bir tık daha en büyük görünüm.</small
      ></span
    >
    <span class="seg">
      <button class:on={data.settings.island.reveal === 'hover'} onclick={() => update(s => (s.island.reveal = 'hover'))}>Üzerine gelince</button>
      <button class:on={data.settings.island.reveal === 'click'} onclick={() => update(s => (s.island.reveal = 'click'))}>Tıklayınca</button>
    </span>
  </div>
  <label class="row">
    <span class="ic"><Icon name="island" /></span>
    <span class="tx"><b>Üst çubuğun boş yerine tıklayınca ayrıntı</b><small>Küçük görünümde simgelerin arasındaki boş siyah alana basmak en büyük görünümü açar; imleci en üste atıp tıklamak yeter.</small></span>
    <input class="switch" type="checkbox" role="switch" checked={data.settings.island.barOpen} onchange={e => update(s => (s.island.barOpen = e.currentTarget.checked))} />
  </label>
  <label class="row">
    <span class="ic"><Icon name="music" /></span>
    <span class="tx"><b>Ses tuşlarında Zody göstergesi</b><small>Ses tuşlarına basınca çentik 1,5 saniyeliğine ses çubuğu olur; Windows'un kendi ses penceresi çıkmaz. Kapalıyken ses tuşları tamamen Windows'ta kalır, Zodyvel tuşları dinlemez. Ada tam ekranda gizliyken ya da açıkken Windows'un göstergesi çıkar.</small></span>
    <input class="switch" type="checkbox" role="switch" checked={data.settings.island.volumeOsd} onchange={e => update(s => (s.island.volumeOsd = e.currentTarget.checked))} />
  </label>
  <div class="row">
    <span class="ic"><Icon name="music" /></span>
    <span class="tx"><b>Üzerine gelince</b><small>Otomatik: müzik çalıyorsa mini oynatıcı, değilse AI. Son bıraktığım: en son hangi bölümdeysen (ör. Bugün) orada açılır. Bekleyen onay her zaman AI'yı açar.</small></span>
    <span class="seg">
      <button class:on={data.settings.island.hover === 'auto'} onclick={() => update(s => (s.island.hover = 'auto'))}>Otomatik</button>
      <button class:on={data.settings.island.hover === 'ai'} onclick={() => update(s => (s.island.hover = 'ai'))}>Hep AI</button>
      <button class:on={data.settings.island.hover === 'last'} onclick={() => update(s => (s.island.hover = 'last'))}>Son bıraktığım</button>
    </span>
  </div>
</section>

<section class="group">
  <h3>Ekran ve boyut</h3>
  <div class="row">
    <span class="ic"><Icon name="pc" /></span>
    <span class="tx"
      ><b>Hangi ekranda</b><small
        >Birden çok ekran varsa ada hangisinin üst ortasında dursun. İmlecin olduğu ekran: ada kapalıyken imleç başka ekrana geçince ada da oraya
        taşınır.</small
      ></span
    >
    <span class="seg" role="radiogroup" aria-label="Hangi ekranda">
      {#each MODES as m (m.mode)}
        <button role="radio" aria-checked={island.screen === m.mode} class:on={island.screen === m.mode} onclick={() => pickMode(m.mode)}>{m.label}</button>
      {/each}
    </span>
  </div>
  {#if island.screen === 'fixed'}
    <div class="row">
      <span class="ic blank" aria-hidden="true"></span>
      <span class="tx"
        ><b>Ekran</b><small class:warn={missing}
          >{missing ? 'Seçilen ekran şu an bağlı değil; ada birincil ekranda. Ekran takılınca oraya döner.' : 'Ekran çıkarılırsa ada birincil ekrana geçer, takılınca geri döner.'}</small
        ></span
      >
      <select
        class="pick"
        aria-label="Ekran"
        value={island.screenId}
        onchange={e => {
          const id = e.currentTarget.value;
          void update(s => (s.island.screenId = id));
        }}
      >
        {#if missing}
          <option value={island.screenId}>Bağlı olmayan ekran</option>
        {/if}
        {#each screens as screen (screen.id)}
          <option value={screen.id}>{screen.label}{screen.primary ? ' (birincil)' : ''}</option>
        {/each}
      </select>
    </div>
  {/if}
  <div class="row">
    <span class="ic"><Icon name="island" /></span>
    <span class="tx"><b>Boyut</b><small>Ada ve içindeki her şey bu oranda büyür ya da küçülür; yazılar keskin kalır. Windows ölçeğinin üstüne eklenir.</small></span>
    <input
      class="range"
      type="range"
      min={SIZE.min}
      max={SIZE.max}
      step={SIZE.step}
      value={island.size}
      aria-label="Ada boyutu"
      oninput={e => {
        const size = Number(e.currentTarget.value);
        void update(s => (s.island.size = size));
      }}
    />
    <output>%{island.size}</output>
  </div>
</section>

<section class="group">
  <h3>Selam</h3>
  <label class="row">
    <span class="ic"><Icon name="motion" /></span>
    <span class="tx"><b>Üzerine gelince önce selam</b><small>Zody şeritten kafasını çıkarır (5 dakikada bir el sallar), sonra küçük görünüm açılır; tıklayınca hemen açılır. Kapalıyken doğrudan küçük görünüm. "Tıklayınca" açılışta Zody her zaman çıkar.</small></span>
    <input class="switch" type="checkbox" role="switch" checked={data.settings.island.peek} onchange={e => update(s => (s.island.peek = e.currentTarget.checked))} />
  </label>
  <label class="row">
    <span class="ic"><Icon name="spark" /></span>
    <span class="tx"><b>Açılışta selam</b><small>Zodyvel başlayınca Zody iki saniyeliğine çıkıp el sallar.</small></span>
    <input class="switch" type="checkbox" role="switch" checked={data.settings.island.greet} onchange={e => update(s => (s.island.greet = e.currentTarget.checked))} />
  </label>
  <div class="row">
    <span class="ic"><Icon name="info" /></span>
    <span class="tx"><b>Tanıtım turu</b><small>Adanın nasıl kullanıldığını dört kısa adımda anlatır; adada hemen açılır.</small></span>
    <button class="btn" onclick={() => update(s => (s.island.welcomed = false))}>Turu yeniden göster</button>
  </div>
</section>

<style>
  .range {
    width: 150px;
    flex: none;
    accent-color: var(--accent);
  }
  .ic.blank {
    visibility: hidden;
  }
  .pick {
    flex: none;
    max-width: 260px;
    height: 30px;
    padding: 0 8px;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--surface-2);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-12);
  }
  output {
    width: 56px;
    flex: none;
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--text-2);
  }
</style>
