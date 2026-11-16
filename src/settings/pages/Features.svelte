<script lang="ts">
  // Özellikler: modüllerin içindeki küçük eklemeler tek tek açılıp kapanır. Kapalı özellik veri okumaz,
  // olay dinlemez ve adada yer kaplamaz. Yeni her özellik buraya bir satır olarak eklenir.
  import { onMount } from 'svelte';
  import { call, callOrThrow, inTauri } from '../../lib/ipc';
  import { data } from '../../lib/store.svelte';
  import { defaultFeatures, type Features, type SetupPreview, type StatusInspection } from '../../lib/types';
  import Icon from '../Icon.svelte';
  import { go, update } from '../state.svelte';

  const f = $derived(data.settings.features);
  const flashOff = $derived(!data.settings.island.flash);
  const pcOff = $derived(!data.settings.modules.system);
  const musicOff = $derived(!data.settings.modules.music);
  const todayOff = $derived(!data.settings.modules.today);
  const modesOff = $derived(!data.settings.modules.modes);
  const classic = $derived(data.settings.appearance.look !== 'new');
  const changed = $derived(JSON.stringify(f) !== JSON.stringify(defaultFeatures()));

  // Pil var mı: sayfa açılınca bir kez sorulur (masaüstünde pil satırları yalnız bilgi verir).
  let hasBattery = $state<boolean | null>(inTauri ? null : true);
  onMount(() => {
    if (inTauri) void call<boolean>('power_present').then(p => (hasBattery = p ?? false));
  });

  const set = <K extends keyof Features>(key: K, value: Features[K]) => update(s => (s.features[key] = value));

  // Plan limiti: Claude Code durum satırı köprüsü. Ayar dosyası yalnız önizleme ve "Uygula" ile değişir.
  let status = $state<StatusInspection | null>(null);
  let preview = $state<SetupPreview | null>(null);
  let busy = $state(false);
  let limitError = $state<string | null>(null);
  let limitNotice = $state<string | null>(null);
  onMount(() => {
    if (inTauri) void call<StatusInspection>('statusline_inspect').then(s => (status = s ?? null));
  });

  async function prepare(action: 'install' | 'remove') {
    busy = true;
    limitError = limitNotice = null;
    try {
      preview = await callOrThrow<SetupPreview>('statusline_prepare', { action });
    } catch (e) {
      limitError = String(e);
    } finally {
      busy = false;
    }
  }

  async function apply() {
    if (!preview) return;
    busy = true;
    limitError = null;
    try {
      await callOrThrow('connections_apply', { id: preview.id });
      status = (await call<StatusInspection>('statusline_inspect')) ?? null;
      limitNotice = status?.installed
        ? 'Kaydedildi. Claude Code değişikliği kendiliğinden algılar; limit, oturumdaki ilk yanıttan sonra görünür.'
        : 'Durum satırı eski hâline döndü.';
      preview = null;
    } catch (e) {
      limitError = String(e);
    } finally {
      busy = false;
    }
  }

  function toggleLimit(on: boolean) {
    set('planLimit', on);
    preview = null;
    limitNotice = null;
    // Açılınca köprü kurulu değilse önizleme hemen gösterilir.
    if (on && status && !status.installed && !status.error) void prepare('install');
  }
  const MINUTES = [5, 10, 20] as const;
</script>

<header class="page-head">
  <h1>Özellikler</h1>
  <p>Her ek özellik ayrı ayrı açılıp kapanır. Kapalı bir özellik hiçbir veri okumaz, bildirim göstermez ve adada yer kaplamaz.</p>
</header>

<section class="group">
  <h3>Ada</h3>
  <label class="row">
    <span class="ic"><Icon name="island" /></span>
    <span class="tx">
      <b>En son hareketli bölümde aç</b>
      <small>Ada açılırken en son bir şey olan bölüm gelir: biten ya da değişen Claude oturumu, yeni mail veya kırılan CI, çalmaya başlayan şarkı, dolan zamanlayıcı. Bekleyen onay her zaman önce gelir; kapalıyken müzik çalıyorsa müzik, değilse AI açılır. Ada › Üzerine gelince “Son bıraktığım” seçiliyse o geçerlidir.</small>
    </span>
    <input class="switch" type="checkbox" role="switch" checked={f.recentTab} onchange={e => set('recentTab', e.currentTarget.checked)} />
  </label>
  <label class="row">
    <span class="ic"><Icon name="spark" /></span>
    <span class="tx">
      <b>Zody ile konuş</b>
      <small>Üst çubuğun en sağında Zody sekmesi: Zody'ye yazarak komut verirsin ("25 dk zamanlayıcı", "müziği durdur", "yarın 10'da toplantı ekle", "Claude ne yapıyor?"). Şimdilik kurallarla anlar, internete çıkmaz. Takvime ekleme, Wi-Fi/Bluetooth gibi işler önce onay kartı gösterir; onayı yalnız düğme verir.</small>
      {#if f.zodyChat && classic}
        <span class="warn">Yalnız Yeni görünümde. <button class="link" onclick={() => go('gorunum')}>Görünüm</button></span>
      {/if}
    </span>
    <input class="switch" type="checkbox" role="switch" checked={f.zodyChat} onchange={e => set('zodyChat', e.currentTarget.checked)} />
  </label>
</section>

<section class="group">
  <h3>AI uyarıları</h3>
  <label class="row">
    <span class="ic"><Icon name="spark" /></span>
    <span class="tx">
      <b>Bitti özeti</b>
      <small>“Bitti” bildiriminde turun ne kadar sürdüğü ve kaç dosyanın değiştiği yazar: “Bitti · 4 dk 12 sn”, “Claude · Psrc · 3 dosya”.</small>
      {#if flashOff && f.doneSummary}
        <span class="warn">Bitti bildirimi kapalı; özet görünmez. <button class="link" onclick={() => go('bildirim')}>Bildirimler</button></span>
      {/if}
    </span>
    <input class="switch" type="checkbox" role="switch" checked={f.doneSummary} onchange={e => set('doneSummary', e.currentTarget.checked)} />
  </label>
  <div class="row">
    <span class="ic"><Icon name="clock" /></span>
    <label class="tx" for="wait-alert">
      <b>Bekleme hatırlatması</b>
      <small>Bir onay, soru ya da senden yanıt uzun süre beklerse çentikte bir kez hatırlatır ve ses çalar: “12 dk'dır onay bekliyor”.</small>
    </label>
    {#if f.waitAlert}
      <span class="seg" role="radiogroup" aria-label="Hatırlatma süresi">
        {#each MINUTES as m (m)}
          <button role="radio" aria-checked={f.waitAlertMin === m} class:on={f.waitAlertMin === m} onclick={() => set('waitAlertMin', m)}>{m} dk</button>
        {/each}
      </span>
    {/if}
    <input id="wait-alert" class="switch" type="checkbox" role="switch" checked={f.waitAlert} onchange={e => set('waitAlert', e.currentTarget.checked)} />
  </div>
</section>

<section class="group">
  <h3>Claude</h3>
  <label class="row">
    <span class="ic"><Icon name="plan" /></span>
    <span class="tx">
      <b>Plan onayı adada</b>
      <small>Claude plan modunda planını sunduğunda plan adada açılır; okuyup onaylarsın, reddedersin ya da ne değişmesi gerektiğini yazıp geri gönderirsin. Kapalıyken plan yalnız Claude penceresinde sorulur.</small>
    </span>
    <input class="switch" type="checkbox" role="switch" checked={f.planReview} onchange={e => set('planReview', e.currentTarget.checked)} />
  </label>
  <label class="row">
    <span class="ic"><Icon name="ai" /></span>
    <span class="tx">
      <b>Ajana not</b>
      <small>AI ayrıntısından çalışan Claude oturumuna not bırakırsın: "Sonra" notu tur biterken yeni istek olarak gider, "Şimdi yönlendir" bir sonraki adımı durdurup notu iletir. Yalnız Claude Code'da, adadan onay modunda çalışır.</small>
    </span>
    <input class="switch" type="checkbox" role="switch" checked={f.agentNotes} onchange={e => set('agentNotes', e.currentTarget.checked)} />
  </label>
  <label class="row">
    <span class="ic"><Icon name="ai" /></span>
    <span class="tx">
      <b>Yazma sahnesi</b>
      <small>Simge çubuğundaki ✎ ya da Ctrl+Alt+Y (Kısayollar'dan değişir) mini adada yazma sahnesini açar: açık bir oturuma not ya da yeni tur yazarsın. Ajana not artık buradan gider; ayrıntıda sıradaki notlar kalır.</small>
      {#if f.writeScene && classic}
        <span class="warn">Yalnız Yeni görünümde. <button class="link" onclick={() => go('gorunum')}>Görünüm</button></span>
      {/if}
    </span>
    <input class="switch" type="checkbox" role="switch" checked={f.writeScene} onchange={e => set('writeScene', e.currentTarget.checked)} />
  </label>
  <label class="row">
    <span class="ic"><Icon name="folder" /></span>
    <span class="tx">
      <b>Yeni görev ve devam</b>
      <small>Yazma sahnesinde "+ Yeni görev": klasör seçip görevi yazarsın, Claude Code arka planda başlar, adaya numaralı bir Zody olarak düşer. "Kapanmış ▾" son oturumları kaldığı yerden sürdürür. Planından gider.</small>
      {#if f.newTask && !f.writeScene}
        <span class="warn">Yazma sahnesi kapalı; görünmez.</span>
      {/if}
    </span>
    <input class="switch" type="checkbox" role="switch" checked={f.newTask} onchange={e => set('newTask', e.currentTarget.checked)} />
  </label>
  <label class="row">
    <span class="ic"><Icon name="clock" /></span>
    <span class="tx">
      <b>Kartı sonraya katla</b>
      <small>Onay ve soru kartında "Sonra ⌄": kart cevaplanmadan küçülür, istek beklemeye devam eder; simge çubuğunda "1 onay bekliyor · göster" kalır.</small>
      {#if f.foldCard && classic}
        <span class="warn">Yalnız Yeni görünümde. <button class="link" onclick={() => go('gorunum')}>Görünüm</button></span>
      {/if}
    </span>
    <input class="switch" type="checkbox" role="switch" checked={f.foldCard} onchange={e => set('foldCard', e.currentTarget.checked)} />
  </label>
  <label class="row">
    <span class="ic"><Icon name="key" /></span>
    <span class="tx">
      <b>Onaya ve terminale git kısayolları</b>
      <small>Ctrl+Alt+O bekleyen onay ya da soru kartını adada açar; Ctrl+Alt+T onay bekleyen (yoksa seni bekleyen) oturumun penceresini öne getirir. İkisi de Kısayollar'dan değişir ya da kaldırılır.</small>
      {#if f.jumpKeys && classic}
        <span class="warn">Yalnız Yeni görünümde. <button class="link" onclick={() => go('gorunum')}>Görünüm</button></span>
      {/if}
    </span>
    <input class="switch" type="checkbox" role="switch" checked={f.jumpKeys} onchange={e => set('jumpKeys', e.currentTarget.checked)} />
  </label>
  <label class="row">
    <span class="ic"><Icon name="chart" /></span>
    <span class="tx">
      <b>Model ve bağlam doluluğu</b>
      <small>AI ayrıntısında oturumun modeli ve bağlamının ne kadar dolduğu. Claude'un oturum kaydının yalnız sonu, yalnız ayrıntı açıkken okunur.</small>
    </span>
    <input class="switch" type="checkbox" role="switch" checked={f.agentContext} onchange={e => set('agentContext', e.currentTarget.checked)} />
  </label>
  <label class="row">
    <span class="ic"><Icon name="bell" /></span>
    <span class="tx">
      <b>Döngü uyarısı</b>
      <small>Ajan aynı komutu ya da aynı dosya düzenlemesini üst üste üç kez deneyip başaramazsa AI ayrıntısında sarı bir uyarı şeridi çıkar.</small>
    </span>
    <input class="switch" type="checkbox" role="switch" checked={f.loopAlert} onchange={e => set('loopAlert', e.currentTarget.checked)} />
  </label>
  <div class="row">
    <span class="ic"><Icon name="chart" /></span>
    <label class="tx" for="plan-limit">
      <b>Plan limiti hapı</b>
      <small>AI ayrıntısında “Claude %73”: 5 saatlik ve haftalık Claude limitinin ne kadarını kullandığın; üstüne gelince sıfırlanma saatleri. Bilgiyi Claude Code'un durum satırına eklenen küçük bir köprü verir; kendi durum satırın varsa olduğu gibi çalışmaya devam eder. Yalnız Pro/Max aboneliğinde.</small>
      {#if f.planLimit && classic}
        <span class="warn">Yalnız Yeni görünümde. <button class="link" onclick={() => go('gorunum')}>Görünüm</button></span>
      {/if}
    </label>
    <input id="plan-limit" class="switch" type="checkbox" role="switch" checked={f.planLimit} onchange={e => toggleLimit(e.currentTarget.checked)} />
  </div>
  {#if inTauri && status && (f.planLimit || status.installed || preview)}
    <div class="more">
      {#if status.error}
        <p class="err">{status.path}: {status.error}</p>
      {:else if f.planLimit && status.installed}
        <p class="sub">
          <span class="pill ok">Durum satırına bağlı</span>
          {#if status.original}Kendi durum satırın da çalışıyor: <code class="mono">{status.original}</code>{/if}
        </p>
      {:else if f.planLimit}
        <div class="line">
          <span class="warn">Claude Code'a bağlı değil; hap boş kalır.</span>
          <span class="sp"></span>
          {#if !preview}<button class="btn primary" disabled={busy || !status.hookReady} onclick={() => prepare('install')}>Bağla…</button>{/if}
        </div>
      {:else if status.installed}
        <div class="line">
          <span class="sub">Özellik kapalı ama durum satırı köprüsü hâlâ kurulu.</span>
          <span class="sp"></span>
          {#if !preview}<button class="btn" disabled={busy} onclick={() => prepare('remove')}>Durum satırını geri al…</button>{/if}
        </div>
      {/if}
      {#if preview}
        <div class="preview">
          <p class="ptitle">{preview.path} dosyasında şu değişiklikler yapılacak:</p>
          <ul>
            {#each preview.changes as change, i (i)}<li>{change}</li>{/each}
          </ul>
          <details>
            <summary>Dosyanın yeni hâlini göster</summary>
            <pre class="mono">{preview.content}</pre>
          </details>
          <div class="line">
            <span class="sp"></span>
            <button class="btn" disabled={busy} onclick={() => (preview = null)}>Vazgeç</button>
            <button class="btn primary" disabled={busy} onclick={apply}>Uygula</button>
          </div>
        </div>
      {/if}
      {#if limitError}<p class="err">{limitError}</p>{/if}
      {#if limitNotice}<p class="sub">{limitNotice}</p>{/if}
    </div>
  {/if}
  <label class="row">
    <span class="ic"><Icon name="chart" /></span>
    <span class="tx">
      <b>Kullanım ve maliyet özeti</b>
      <small>AI ayrıntısında bugünün ve bu haftanın token kullanımı ve tahmini API maliyeti; projeler, modeller ve geçen haftanın özeti. Claude'un bilgisayardaki kayıtlarından, yalnız AI paneli açıkken okunur; internete bir şey gönderilmez.</small>
    </span>
    <input class="switch" type="checkbox" role="switch" checked={f.usage} onchange={e => set('usage', e.currentTarget.checked)} />
  </label>
  <label class="row" class:disabled={!f.usage}>
    <span class="ic"><Icon name="today" /></span>
    <span class="tx">
      <b>Pazartesi haftalık özet</b>
      <small>Haftanın ilk açılışında çentikte bir kez geçen hafta: çalışma süresi, oturum ve dosya sayısı, en yoğun gün.</small>
      {#if !f.usage && f.weeklyRecap}<span class="warn">Kullanım özeti kapalı; haftalık özet de gösterilmez.</span>{/if}
    </span>
    <input class="switch" type="checkbox" role="switch" checked={f.weeklyRecap} onchange={e => set('weeklyRecap', e.currentTarget.checked)} />
  </label>
</section>

<section class="group">
  <h3>Pil</h3>
  {#if hasBattery === false}
    <p class="note">Bu bilgisayarda pil yok; pil özellikleri açık kalsa da hiçbir şey göstermez ve okumaz.</p>
  {/if}
  <label class="row" class:disabled={hasBattery === false}>
    <span class="ic"><Icon name="battery" /></span>
    <span class="tx">
      <b>Bilgisayar ekranında pil</b>
      <small>Bilgisayar bölümünde dördüncü gösterge: pil yüzdesi, şarj durumu ve Windows'un tahmini kalan süresi.</small>
      {#if pcOff && f.battery}
        <span class="warn">Bilgisayar modülü kapalı. <button class="link" onclick={() => go('moduller')}>Modüller</button></span>
      {/if}
    </span>
    <input class="switch" type="checkbox" role="switch" checked={f.battery} onchange={e => set('battery', e.currentTarget.checked)} />
  </label>
  <label class="row" class:disabled={hasBattery === false}>
    <span class="ic"><Icon name="bell" /></span>
    <span class="tx">
      <b>Pil bildirimleri</b>
      <small>Şarja takınca ya da çıkarınca, pil %20 ve %10'a inince çentikte kısa not. Windows değişikliği bildirir; sürekli ölçüm yapılmaz.</small>
    </span>
    <input class="switch" type="checkbox" role="switch" checked={f.batteryAlerts} onchange={e => set('batteryAlerts', e.currentTarget.checked)} />
  </label>
</section>

<section class="group">
  <h3>Müzik ve Bugün</h3>
  <label class="row">
    <span class="ic"><Icon name="music" /></span>
    <span class="tx">
      <b>Kapaktan renk</b>
      <small>Müzik kartı ve oynat düğmesi çalan parçanın kapak rengini alır. Renk kapak değişince bir kez bulunur; sürekli iş yapılmaz.</small>
      {#if f.coverTint && classic}
        <span class="warn">Yalnız Yeni görünümde. <button class="link" onclick={() => go('gorunum')}>Görünüm</button></span>
      {:else if f.coverTint && musicOff}
        <span class="warn">Müzik modülü kapalı. <button class="link" onclick={() => go('moduller')}>Modüller</button></span>
      {/if}
    </span>
    <input class="switch" type="checkbox" role="switch" checked={f.coverTint} onchange={e => set('coverTint', e.currentTarget.checked)} />
  </label>
  <label class="row">
    <span class="ic"><Icon name="clock" /></span>
    <span class="tx">
      <b>Odak modu</b>
      <small>Modlarda “25/5 odak döngüsü” seçeneği: 25 dk odak, 5 dk mola; dört odaktan sonra 15 dk uzun mola, mod sürdükçe baştan. Her geçişte çentikte kısa not ve zil.</small>
      {#if f.focus && modesOff}
        <span class="warn">Modlar modülü kapalı. <button class="link" onclick={() => go('moduller')}>Modüller</button></span>
      {/if}
    </span>
    <input class="switch" type="checkbox" role="switch" checked={f.focus} onchange={e => set('focus', e.currentTarget.checked)} />
  </label>
</section>

<section class="group">
  <h3>Dosyalar</h3>
  <label class="row">
    <span class="ic"><Icon name="folder" /></span>
    <span class="tx">
      <b>Dosya rafı</b>
      <small>Bir dosyayı ekranın üstündeki adaya sürükle, rafta dursun; sonra raftan tutup başka bir uygulamaya bırak. Dosyalar kopyalanmaz ya da taşınmaz, raf yalnız yerlerini hatırlar. Sürükleme yokken hiçbir iş yapılmaz.</small>
    </span>
    <input class="switch" type="checkbox" role="switch" checked={f.shelf} onchange={e => set('shelf', e.currentTarget.checked)} />
  </label>
  <label class="row">
    <span class="ic"><Icon name="ai" /></span>
    <span class="tx">
      <b>Raftan Claude'a sor</b>
      <small>Raftaki bir dosyanın üstüne gelince "Sor": yazma sahnesi o dosya ekli açılır, sorunu açık bir oturuma ya da yeni göreve gönderirsin. Dosyanın kendisi gönderilmez, Claude yolunu alıp kendisi okur.</small>
      {#if f.shelfAsk && classic}
        <span class="warn">Yalnız Yeni görünümde. <button class="link" onclick={() => go('gorunum')}>Görünüm</button></span>
      {:else if f.shelfAsk && (!f.shelf || !f.writeScene)}
        <span class="warn">Dosya rafı ve Yazma sahnesi açık olmalı.</span>
      {/if}
    </span>
    <input class="switch" type="checkbox" role="switch" checked={f.shelfAsk} onchange={e => set('shelfAsk', e.currentTarget.checked)} />
  </label>
</section>

<section class="group">
  <h3>Kaynaklar</h3>
  <label class="row">
    <span class="ic"><Icon name="grid" /></span>
    <span class="tx">
      <b>Kaynaklar düzeni</b>
      <small>AI sekmesi “Kaynaklar” olur: Mail ve GitHub gibi AI dışı kaynaklar adada, o an en önemlisi solda. Hangi kaynakların açık olduğu ve bağlantıları Kaynaklar sayfasında.</small>
      {#if f.sources && classic}
        <span class="warn">Yalnız Yeni görünümde. <button class="link" onclick={() => go('gorunum')}>Görünüm</button></span>
      {:else}
        <span class="warn"><button class="link" onclick={() => go('kaynaklar')}>Kaynaklar sayfası</button></span>
      {/if}
    </span>
    <input class="switch" type="checkbox" role="switch" checked={f.sources} onchange={e => set('sources', e.currentTarget.checked)} />
  </label>
  <label class="row">
    <span class="ic"><Icon name="link" /></span>
    <span class="tx">
      <b>Bilgisayarımdaki depolar (yerel commit)</b>
      <small>GitHub kutucuğunda “Depolarım”: bilgisayardaki depolarda commitlenmemiş değişiklikler, onaylı Commit ve Push. Yalnız istenince bakar (Depolarım açılınca, Yenile’de); arka planda çalışmaz. Klasörler Kaynaklar › GitHub’da.</small>
      <span class="warn"><button class="link" onclick={() => go('kaynaklar')}>Kaynaklar sayfası</button></span>
    </span>
    <input
      class="switch"
      type="checkbox"
      role="switch"
      checked={f.localGit}
      onchange={e => {
        const on = e.currentTarget.checked;
        void update(s => {
          s.features.localGit = on;
          if (on) s.features.sources = s.features.sourceGithub = true;
        });
      }}
    />
  </label>
</section>

<div class="foot">
  <button class="btn" disabled={!changed} onclick={() => update(s => (s.features = defaultFeatures()))}>Varsayılana dön</button>
</div>

<style>
  .tx .warn {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  label.tx {
    cursor: pointer;
  }
  .note {
    margin: 0;
    padding: 6px 16px 4px;
    color: var(--text-2);
    font-size: var(--fs-12);
  }
  .row.disabled .tx b {
    color: var(--text-2);
  }
  .foot {
    display: flex;
    justify-content: flex-end;
  }
  .more {
    display: grid;
    gap: 8px;
    padding: 0 16px 14px 64px;
  }
  .more p {
    margin: 0;
  }
  .sub {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    font-size: var(--fs-12);
    color: var(--text-2);
  }
  .sub code {
    word-break: break-all;
    font-size: var(--fs-11);
  }
  .line {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .sp {
    flex: 1;
  }
  .preview {
    display: grid;
    gap: 6px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--surface-2);
  }
  .ptitle {
    font-weight: 600;
    font-size: var(--fs-12-5);
    word-break: break-all;
  }
  ul {
    margin: 0;
    padding-left: 18px;
    font-size: var(--fs-12);
    color: var(--text-2);
    display: grid;
    gap: 3px;
  }
  details {
    font-size: var(--fs-12);
    color: var(--text-2);
  }
  pre {
    max-height: 220px;
    overflow: auto;
    margin: 6px 0 0;
    padding: 8px;
    border-radius: 8px;
    background: var(--island);
    font-size: var(--fs-11);
    color: var(--text);
    user-select: text;
  }
</style>
