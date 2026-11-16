<script lang="ts">
  // Bildirimler: ada kapalıyken çentiğin habere dönüşmesi ve sesler (set, olaylar, kendi dosyan).
  import { onMount } from 'svelte';
  import { call, callOrThrow } from '../../lib/ipc';
  import { data } from '../../lib/store.svelte';
  import { defaultSettings, type NotchHover, type Settings, type SoundCue } from '../../lib/types';
  import Icon, { type IconName } from '../Icon.svelte';
  import { update } from '../state.svelte';

  const cues: { id: SoundCue; icon: IconName; title: string; text: string }[] = [
    { id: 'attention', icon: 'ai', title: 'Onay ya da soru', text: 'Bir ajan izin isteyince ya da soru sorunca.' },
    { id: 'done', icon: 'spark', title: 'Bitti', text: 'Bir ajanın turu bitince.' },
    { id: 'error', icon: 'info', title: 'Hata', text: 'Bir ajan hata verince.' },
    { id: 'timer', icon: 'today', title: 'Zodymetre ve mod', text: 'Zodymetre dolunca (kendi ses anahtarı da açıksa), mod bitince ve odak aşaması değişince.' },
    { id: 'hello', icon: 'power', title: 'Açılış selamı', text: 'Zodyvel açılınca Zody kısa bir selam verir.' },
  ];

  const sounds = $derived(data.settings.sounds);
  const notch = $derived(data.settings.notch ?? defaultSettings.notch);
  const hovers: { id: NotchHover; label: string }[] = [
    { id: 'peek', label: 'Ara boy' },
    { id: 'open', label: 'Doğrudan aç' },
    { id: 'click', label: 'Tıklayınca' },
    { id: 'mini', label: 'Mini pencere' },
  ];
  const minis: { id: 'miniAi' | 'miniMusic' | 'miniTimer'; icon: IconName; title: string; text: string }[] = [
    { id: 'miniAi', icon: 'ai', title: 'Mini AI', text: 'Sohbet adı, son cevaptan iki satır, aşamalar; Durdur, Ayrıntı, Kapat. Bitti ve hata bildiriminde de açılır.' },
    { id: 'miniMusic', icon: 'music', title: 'Mini Oynatıcı', text: 'Kapak, şarkı, önceki / oynat / sonraki ve ilerleme.' },
    { id: 'miniTimer', icon: 'clock', title: 'Mini Kronometre', text: 'Halka ve kalan süre; Duraklat, +5 dk, Bitir. Birden çok Zodymetre varsa üstte sekmeler.' },
  ];
  const kinds: { id: 'timer' | 'mode' | 'ai' | 'music'; icon: IconName; title: string; text: string }[] = [
    { id: 'timer', icon: 'clock', title: 'Zodymetre', text: 'Çalışırken halka ve kalan süre; en çok üçü yan yana.' },
    { id: 'mode', icon: 'today', title: 'Mod', text: 'Açık mod, renginde; kalan (süresizse geçen) süre.' },
    { id: 'ai', icon: 'ai', title: 'AI çalışıyor', text: 'Ne yaptığı (Düzenliyor auth.ts) ve kaç dakikadır.' },
    { id: 'music', icon: 'music', title: 'Müzik', text: 'Çalan şarkı; başka etkinlik varken sağda yalnız kapak.' },
  ];
  const ensure = (s: Settings) => (s.notch ??= structuredClone(defaultSettings.notch));
  /** Windows "Rahatsız etme" şu an açık mı (sayfa açılınca ve pencereye dönünce okunur). */
  let quiet = $state(false);
  let errors = $state<Partial<Record<SoundCue, string>>>({});
  let picking: SoundCue | null = null;
  let input: HTMLInputElement;

  const readQuiet = () => void call<boolean>('sound_quiet').then(q => (quiet = q ?? false));
  onMount(readQuiet);

  function pick(cue: SoundCue) {
    picking = cue;
    errors[cue] = undefined;
    input.value = '';
    input.click();
  }

  async function upload() {
    const cue = picking;
    const file = input.files?.[0];
    if (!cue || !file) return;
    try {
      const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
      data.settings = await callOrThrow<Settings>('sound_upload', { kind: cue, name: file.name, bytes });
    } catch (e) {
      errors[cue] = String(e);
    }
  }
</script>

<svelte:window onfocus={readQuiet} />

<header class="page-head">
  <h1>Bildirimler</h1>
  <p>Ada kapalıyken çentik, o an süren en önemli şeyi tek satırda gösterir: onay, Bitiş, Zodymetre, açık mod, çalışan AI ya da müzik. Üzerine gelince ya da tıklayınca ilgili yer açılır.</p>
</header>

<section class="group">
  <label class="row">
    <span class="ic"><Icon name="sound" /></span>
    <span class="tx">
      <b>Sesler</b>
      <small>Tüm seslerin anahtarı. {data.settings.shortcuts.mute ? `${data.settings.shortcuts.mute} ya da tepsiden de kapatılır.` : 'Tepsiden de kapatılır.'}</small>
    </span>
    <input class="switch" type="checkbox" role="switch" checked={data.settings.island.sound} onchange={e => update(s => (s.island.sound = e.currentTarget.checked))} />
  </label>
  <label class="row">
    <span class="ic"><Icon name="spark" /></span>
    <span class="tx"><b>"Bitti" ve "Hata" bildirimi</b><small>Bir ajanın turu bitince yeşil, hata verince kırmızı Zody 5 saniye görünür.</small></span>
    <input class="switch" type="checkbox" role="switch" checked={data.settings.island.flash} onchange={e => update(s => (s.island.flash = e.currentTarget.checked))} />
  </label>
</section>

<section class="group">
  <h3>Çentik</h3>
  <label class="row">
    <span class="ic"><Icon name="island" /></span>
    <span class="tx">
      <b>Her zaman açık ada</b>
      <small>Açıkken Zody'nin gözleri küçük adada hep görünür; Zodymetre, çalışan AI ve müzik yanında yer alır. Kapalıyken Zody yalnız bir şey olunca çıkar (onay sarı, bitti yeşil) ve sonra kaybolur, geriye siyah çentik kalır.</small>
    </span>
    <input class="switch" type="checkbox" role="switch" checked={notch.live} onchange={e => update(s => (ensure(s).live = e.currentTarget.checked))} />
  </label>
  <div class="row">
    <span class="ic"><Icon name="motion" /></span>
    <span class="tx">
      <b>Fare çentiğe gelince</b>
      <small>Ara boy: üzerine gelinen hap genişleyip ayrıntıyı yazar (AI'da süre ve anlık durum), tıklayınca ilgili sekme açılır. Mini pencere: hap düğmeli küçük bir pencereye büyür, fare çıkınca kapanır; aşağıda kapalı olan türler ara boyda kalır. Onay ve soru her zaman doğrudan açılır.</small>
    </span>
    <span class="seg">
      {#each hovers as h (h.id)}
        <button class:on={notch.hover === h.id} onclick={() => update(s => (ensure(s).hover = h.id))}>{h.label}</button>
      {/each}
    </span>
  </div>
  {#if notch.hover === 'mini'}
    {#each minis as k (k.id)}
      <label class="row">
        <span class="ic"><Icon name={k.icon} /></span>
        <span class="tx"><b>{k.title}</b><small>{k.text}</small></span>
        <input class="switch" type="checkbox" role="switch" aria-label={k.title} checked={notch[k.id] ?? true} onchange={e => update(s => (ensure(s)[k.id] = e.currentTarget.checked))} />
      </label>
    {/each}
  {/if}
  {#each kinds as k (k.id)}
    <label class="row" class:off={!notch.live}>
      <span class="ic"><Icon name={k.icon} /></span>
      <span class="tx"><b>{k.title}</b><small>{k.text}</small></span>
      <input class="switch" type="checkbox" role="switch" aria-label={k.title} disabled={!notch.live} checked={notch[k.id]} onchange={e => update(s => (ensure(s)[k.id] = e.currentTarget.checked))} />
    </label>
  {/each}
</section>

<section class="group" class:off={!data.settings.island.sound}>
  <h3>Sesler</h3>
  <div class="row">
    <span class="ic"><Icon name="music" /></span>
    <span class="tx"><b>Ses seti</b><small>Zody: Zodyvel’in kendi kısa sesleri. Windows: bilgisayarının ses düzenindeki sesler.</small></span>
    <span class="seg">
      <button class:on={sounds.pack === 'zody'} onclick={() => update(s => (s.sounds.pack = 'zody'))}>Zody</button>
      <button class:on={sounds.pack === 'windows'} onclick={() => update(s => (s.sounds.pack = 'windows'))}>Windows</button>
    </span>
  </div>
  <label class="row">
    <span class="ic"><Icon name="bell" /></span>
    <span class="tx">
      <b>"Rahatsız etme" açıkken sustur</b>
      <small>Windows’ta Rahatsız etme (ya da Odak yardımı) açıkken ve sunum yaparken hiç ses çalmaz.</small>
      {#if quiet && sounds.respectQuiet}<span class="note">Rahatsız etme şu an açık: sesler çalmıyor.</span>{/if}
    </span>
    <input class="switch" type="checkbox" role="switch" checked={sounds.respectQuiet} onchange={e => update(s => (s.sounds.respectQuiet = e.currentTarget.checked))} />
  </label>
  {#each cues as c (c.id)}
    {@const custom = sounds.custom[c.id]}
    <div class="row">
      <span class="ic"><Icon name={c.icon} /></span>
      <span class="tx">
        <b>{c.title}</b>
        <small>{c.text}</small>
        {#if custom}<small class="file mono" title={custom}>♪ {custom}</small>{/if}
        {#if errors[c.id]}<span class="err">{errors[c.id]}</span>{/if}
      </span>
      <button class="mini" title="Dinle" aria-label="{c.title} sesini dinle" onclick={() => call('sound_preview', { kind: c.id })}>▶</button>
      <button class="mini wide" title="Kendi WAV dosyanı seç" onclick={() => pick(c.id)}>{custom ? 'Değiştir' : 'Dosya…'}</button>
      <button class="mini clear" class:hidden={!custom} title="Kendi sesini kaldır" aria-label="{c.title} için yüklenen sesi kaldır" onclick={() => update(s => delete s.sounds.custom[c.id])}>×</button>
      <input class="switch" type="checkbox" role="switch" aria-label={c.title} checked={sounds[c.id]} onchange={e => update(s => (s.sounds[c.id] = e.currentTarget.checked))} />
    </div>
  {/each}
</section>
<p class="hint">Kendi sesin: kısa bir WAV dosyası (en çok 2 MB). Sesler sistem ses düzeyiyle çalar. Onay istekleri ve dolan zamanlayıcı ses kapalıyken de adada görünür.</p>

<input bind:this={input} type="file" accept=".wav,audio/wav,audio/x-wav" hidden onchange={upload} />

<style>
  .hint {
    margin: 0;
    color: var(--text-3);
    font-size: var(--fs-12);
  }
  .group.off .row,
  .row.off {
    opacity: 0.55;
  }
  .note {
    color: var(--accent-soft);
    font-size: var(--fs-12);
  }
  .file {
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mini {
    height: 28px;
    min-width: 28px;
    padding: 0 8px;
    border-radius: 8px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    color: var(--text-2);
    font-size: var(--fs-12);
    flex: none;
  }
  .mini.wide {
    min-width: 72px;
  }
  .mini:hover {
    background: var(--surface-3);
    color: var(--text);
  }
  .mini.clear {
    background: transparent;
    border-color: transparent;
    font-size: var(--fs-16);
    color: var(--text-3);
  }
  .mini.hidden {
    visibility: hidden;
  }
</style>
