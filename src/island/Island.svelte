<script lang="ts">
  // Ada kabuğu: seviye geçişleri, Rust'tan gelen imleç olayları ve geometri bildirimi.
  import { onMount, untrack } from 'svelte';
  import { fade } from 'svelte/transition';
  import { initial, leaveDelay, step, type IslandInput, type IslandState, type Tab } from '../lib/island-machine';
  import { call, inTauri, on } from '../lib/ipc';
  import { getVersion } from '@tauri-apps/api/app';
  import { agentEvents, batteryNote, dueReminder, FLASH_MS, noticeFor, updateNote, type AgentEvent, type FinishAction, type Notice as NoticeData, type Reminder, type Toast } from '../lib/notice';
  import { defaultSettings, type AgentStatus, type Battery, type ShelfDrag, type ShelfItem, type TaskFailed, type VolumeOsd } from '../lib/types';
  import {
    FINISH_SIZE,
    OSD_W,
    meterItems,
    MINI_CLOSE_MS,
    MINI_IDLE_MS,
    MINI_SIZE,
    miniFor,
    miniHeight,
    NOTCH_H,
    NOTICE_HOLD_MS,
    notchShown,
    notchView,
    notchWidth,
    noticeStays,
    PEEK_H,
    peekable,
    targetKey,
    type LiveItem,
    type NotchTarget,
  } from '../lib/live';
  import {
    context,
    data,
    focusSession,
    loadSettings,
    musicControl,
    loadUsage,
    noteActivity,
    setSystemWatch,
    todayApply,
    unfoldApprovals,
    saveSettings,
    watchAgents,
    watchMusic,
    watchPower,
    watchSettings,
    watchSystem,
    watchShelf,
    watchTasks,
    watchToday,
  } from '../lib/store.svelte';
  import { approvalJump, terminalTarget } from '../lib/jump';
  import { applyTheme } from '../lib/theme';
  import { applyCoverTone } from '../lib/cover';
  import { weekKey, weeklyNote } from '../lib/usage';
  import { openTab as pickOpenTab, saveTab, savedTab } from '../lib/recent';
  import { Greeter } from './greeter.svelte';
  import { insetFor, MORPH_MS, morphArea, NOTCH, radiusFor, SHRINK_MS, sizeFor, type Size } from './levels';
  import IconBar from './parts/IconBar.svelte';
  import Aura from './parts/Aura.svelte';
  import Notch from './parts/Notch.svelte';
  import VolumeOsdView from './parts/VolumeOsd.svelte';
  import NotchMini, { type MiniAction } from './parts/NotchMini.svelte';
  import Finish from './parts/Finish.svelte';
  import { MAX_METERS } from '../lib/today';
  import Peek from './parts/Peek.svelte';
  import Welcome from './parts/Welcome.svelte';
  import { git, loadGit, localGitOn } from '../lib/localgit.svelte';
  import { hub, hubOn, sourceState, watchSources } from '../lib/sources.svelte';
  import type { SourceId } from '../lib/sources';
  import { isFixed } from '../lib/tabs';
  // Sekme ekranları tembel yüklenir (tab-views.ts): kapalı modülün ekranı hiç indirilmez.
  import { load, view as lazy } from './lazy.svelte';
  import { TAB_VIEWS } from './tab-views';
  import { chatHolds, watchChat } from '../lib/chat.svelte';

  let island = $state<IslandState>({ ...initial, tab: savedTab() ?? initial.tab });
  // "Son bıraktığım": bölüm uygulama yeniden açılınca da hatırlansın.
  $effect(() => {
    if (data.settings.island.hover === 'last') saveTab(island.tab);
  });
  let pointerInside = false;
  let leaveTimer: ReturnType<typeof setTimeout> | undefined;

  const ctx = $derived(context());
  // Açılışta gelecek bölüm (recent.ts openTab): bekleyen onay › "Son bıraktığım" (Ayarlar › Ada) ›
  // "En son hareketli bölümde aç" (Özellikler → Ada) › her zamanki kural (ctx.hoverTab).
  const openTab = $derived<Tab>(
    pickOpenTab({
      on: data.settings.features.recentTab,
      last: data.settings.island.hover === 'last' ? island.tab : null,
      ctx,
      activity: data.activity,
      sourcesAt: hubOn() ? hub.urgentAt : 0,
    }),
  );
  // Kapatılan bir modülün bölümü açık kaldıysa ilk görünen bölüme geçilir.
  const tab = $derived<Tab>(ctx.tabs.includes(island.tab) ? island.tab : (ctx.tabs[0] ?? 'ai'));
  // Ada kapalıyken bekleyen onay/soru (istek bitene kadar) ya da bir ajanın bitmesi/hata vermesi
  // (FLASH_MS boyunca) çentiği bildirime dönüştürür.
  let flash = $state<AgentEvent | null>(null);
  let toast = $state<Toast | null>(null);
  let reminder = $state<Reminder | null>(null);
  const features = $derived(data.settings.features);
  const onlyAi = $derived(ctx.modeDef?.onlyAi ?? false);
  const notice = $derived(
    noticeFor({
      approvals: ctx.approvals,
      flash,
      timerDone: ctx.timerDone,
      modeDone: ctx.modeDone,
      // Açık mod "yalnız AI bildirimleri" diyorsa etkinlik ve kısa notlar (pil, özet…) çentiğe gelmez (Faz 5a).
      eventDue: onlyAi ? null : ctx.eventDue,
      toast: onlyAi ? null : toast,
      reminder,
      doneSummary: features.doneSummary,
      lateAfterMin: features.waitAlert ? features.waitAlertMin : null,
      now: Math.max(data.minute, Date.now()),
    }),
  );
  // Canlı çentik (Faz 4, lib/live.ts): olay yoksa süren etkinlik (Zodymetre, AI çalışıyor, müzik) çentikte
  // kalır (Ayarlar › Bildirimler › Çentik). Saniyelik saat yalnız Zodymetre çalışırken ve ada kapalıyken
  // ilerler; başka her durumda dakikalık sayaç (data.minute) yeter.
  const notchPrefs = $derived(data.settings.notch ?? defaultSettings.notch);
  const timerTicks = $derived(
    notchPrefs.live && ((notchPrefs.timer && ctx.meters.some(m => m.endsAt !== null)) || (notchPrefs.mode && ctx.modeRun?.endsAt != null)),
  );
  let liveNow = $state(Date.now());
  $effect(() => {
    if (!timerTicks || island.level > 0) return;
    liveNow = Date.now();
    const id = setInterval(() => (liveNow = Date.now()), 1000);
    return () => clearInterval(id);
  });
  // Olay önce "taze" gelir (renkli akan çerçeve + yazı). Fare NOTICE_HOLD_MS içinde gelmezse söner: onay ve
  // soru yalnız Zody gözlerine döner (istek bitene kadar), bitti/hata ve kısa notlar kaybolur; geriye
  // "her zaman açık ada"da boştaki Zody, kapalıysa siyah şerit kalır (karar 9 Kasım 22:50).
  // Ara boy ya da mini pencerede açık hapın anahtarı (aşağıda notchHover).
  let peekKey = $state<string | null>(null);
  let freshKey = $state<string | null>(null);
  let settleOnLeave = false;
  let freshTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const key = notice?.key ?? null;
    untrack(() => {
      clearTimeout(freshTimer);
      settleOnLeave = false;
      freshKey = key;
      if (key) freshTimer = setTimeout(() => settle(key), NOTICE_HOLD_MS);
    });
  });
  function settle(key: string) {
    // Fareyle bakılıyorsa çıkınca söner.
    if (peekKey === 'notice') return void (settleOnLeave = true);
    if (freshKey !== key) return;
    freshKey = null;
    if (flash?.key === key) flash = null;
  }
  const fresh = $derived(notice !== null && freshKey === notice.key);
  const notchNotice = $derived(notice && (fresh || noticeStays(notice)) ? notice : null);
  const notch = $derived(
    notchView({
      notice: notchNotice,
      prefs: notchPrefs,
      agents: ctx.agents,
      meters: ctx.meters,
      mode: ctx.modeRun ? { run: ctx.modeRun, def: ctx.modeDef } : null,
      music: ctx.music,
      now: Math.max(liveNow, data.minute),
      hold: peekKey,
    }),
  );
  // Ses göstergesi (Faz 10): ses tuşuna basılınca çentik kapsülü 1,5 sn ses çubuğuna döner; yalnız ada kapalıyken.
  let osd = $state<VolumeOsd | null>(null);
  let osdTimer: ReturnType<typeof setTimeout> | undefined;
  function volumeOsd(next: VolumeOsd) {
    if (!data.settings.island.volumeOsd) return;
    osd = next;
    clearTimeout(osdTimer);
    osdTimer = setTimeout(() => (osd = null), OSD_HOLD_MS);
  }
  const OSD_HOLD_MS = 1500;
  const osdOn = $derived(osd !== null && island.level === 0);
  const notchOn = $derived(notchShown(notch) || osdOn);
  // Ara boy: fare bir hapa gelince o hap genişler (Ayarlar: "Fare gelince ara boy"). Genişlik baloncuklarla
  // birlikte içerikten ölçülür; Bitiş bildirimi kendi boyundadır.
  let notchW = $state(0);
  $effect(() => {
    if (island.level > 0 && untrack(() => peekKey)) peekKey = null;
  });
  // Mini pencere (Faz 9, Ayarlar › Bildirimler › Çentik › "Mini pencere"): açık hap o türün mini penceresine
  // büyür; ayarda kapalı olan tür ve mod ara boyda kalır. Tek seferde tek mini pencere (peekKey tektir).
  const peekTarget = $derived.by<NotchTarget | null>(() => {
    if (!peekKey) return null;
    if (peekKey === 'notice') return notch.notice ? 'notice' : null;
    return [...notch.bubbles, ...(notch.lead ? [notch.lead] : [])].find(i => i.key === peekKey) ?? null;
  });
  const miniKind = $derived(peekTarget ? miniFor(peekTarget, notchPrefs, notch.notice) : null);
  // Açık hap çentikten kalktı (tur bitti, şarkı değişti, Zodymetre bitti): ara boy/mini pencere de kapanır.
  $effect(() => {
    if (peekKey && !peekTarget) untrack(() => (peekKey = null));
  });
  const miniTimers = $derived(miniKind === 'timer' ? Math.min(3, meterItems(ctx.meters, Math.max(liveNow, data.minute)).length) : 0);
  const notchSize = $derived.by(() => {
    if (!notchOn) return null;
    if (osdOn) return { w: OSD_W, h: NOTCH_H };
    if (notch.notice?.finish) return FINISH_SIZE;
    if (miniKind) return { w: notchWidth(Math.max(notchW, MINI_SIZE[miniKind].w)), h: miniHeight(miniKind, miniTimers) };
    return { w: notchWidth(notchW || 120), h: peekKey ? PEEK_H : NOTCH_H };
  });
  // Peek, açılış selamı ve tanıtım turu (greeter.svelte.ts): sürerken adanın boyunu o belirler.
  const greeter = new Greeter(() => pointer('hover'));
  const greetSize = $derived(greeter.size(island.level, notchOn));
  // Yazma sahnesi (Özellikler → Yazma sahnesi + AI modülü). Kapanırsa açık sahne mini AI'ya döner.
  const writeOn = $derived(features.writeScene && data.settings.modules.agents);
  $effect(() => {
    if (island.write && !writeOn) untrack(() => send({ type: 'back' }));
  });
  const size = $derived(greetSize ?? sizeFor(island.level, tab, notchSize));
  $effect(() => greeter.sync(data.settings.island.welcomed));
  const reduce = $derived(data.settings.island.reduceMotion);
  // Dalgalı ışık (Aura): adanın altından durum renginde. Onayda sarı, güçlü ve hareketli; bitince yeşil;
  // hatada kırmızı. Çalışırken yok (karar 9 Kasım: turkuaz dalga ekranlara yakışmadı). Yalnız ada açıkken.
  // Karar (9 Kasım): dalga yalnız AI bölümünde ve dosya rafında; müzik, Bugün, Bilgisayar, Saat gibi
  // ekranlarda yok. Sarı onay dalgası AI mini ve ayrıntıda, rafta. Yeşil (bitti) ve kırmızı (hata) yalnız AI
  // ayrıntısında (Kaynaklar'ın Mail/GitHub ayrıntısı açık değilken) ve rafta; mini AI özetinde kalıcı yeşil
  // dalga yok. Yazılar ve düğmeler her zaman dalganın üstünde (z-index); dalga varken kartlar koyulaşır (.lit).
  const aura = $derived.by(() => {
    if (island.level === 0 || tour || (tab !== 'ai' && tab !== 'shelf')) return null;
    if (ctx.approvals.length) return { color: 'var(--amber)', strength: 1, live: true, key: 'attention' };
    // Rafa dosya sürüklenirken hafif, durağan vurgu dalgası (eskiz-paket2 ekran 7).
    if (tab === 'shelf' && data.shelfDrag !== null) return { color: 'var(--accent)', strength: 0.3, live: false, key: 'shelf' };
    if (tab === 'ai' && (island.level !== 2 || hub.detail)) return null;
    if (ctx.mood === 'done') return { color: 'var(--green)', strength: 0.6, live: false, key: 'done' };
    if (ctx.mood === 'error') return { color: 'var(--red)', strength: 0.45, live: false, key: 'error' };
    return null;
  });
  // Tema: ayarlar değişince (settings://changed) ada da yeniden boyanır.
  $effect(() => applyTheme(data.settings.appearance.theme, data.settings.appearance.surface));
  // Kapaktan renk (özellik): kapak değişince bir kez hesaplanır; kapalıyken renk silinir.
  $effect(() => {
    const on = data.settings.features.coverTint && data.settings.modules.music;
    void applyCoverTone(data.artUrl, on);
  });
  // Büyürken yaylı eğri (hafif taşar), küçülürken kısa ve taşmasız. Geçiş tek başına clip-path'tir:
  // ada her zaman pencere boyutunda durur, yalnız kırpma dikdörtgeni değişir; her karede yerleşim
  // (layout) yapılmaz, içerik yeniden boyanmaz.
  let lastSize: Size = NOTCH;
  const grow = $derived.by(() => {
    const s = size;
    const g = s.w > lastSize.w || s.h > lastSize.h;
    lastSize = s;
    return g;
  });
  const morph = $derived(reduce ? 0 : grow ? MORPH_MS : SHRINK_MS);
  const inset = $derived(insetFor(size));
  // Tanıtım turu ya da görünüm sorusu: ada mini boyunda açık durur, seçim yapılana kadar kapanmaz.
  const tour = $derived(greeter.greeting === 'tour');
  const radius = $derived(radiusFor(size));
  // Boştaki siyah şeritte kenar çizgisi yok; çentikte, peek'te ve açık adada 1 px.
  const edge = $derived(island.level === 0 && !notchOn && greetSize === null ? 0 : 1);

  function send(input: IslandInput) {
    const next = step(island, input);
    if (next.level > 0) greeter.cancel();
    if (next.level !== island.level || next.tab !== island.tab || next.write !== island.write) island = next;
  }
  const pick = (t: Tab) => send({ type: 'tab', tab: t });
  const open = () => send({ type: 'open' });
  // Minide düğme olmayan her yere (kartların arasındaki siyah boşluk, Zody, düz metin) tıklamak
  // ayrıntıyı açar; ortadaki karta denk getirmek gerekmez. Simge çubuğu kendi kuralına bakar.
  const NO_OPEN = 'button, a, input, textarea, select, label, summary, [role="button"], [contenteditable], nav, .items';
  function onminiclick(event: MouseEvent) {
    if (island.level !== 1 || island.write || event.button !== 0) return;
    if ((event.target as Element).closest(NO_OPEN) || window.getSelection()?.toString()) return;
    open();
  }
  /** ✎: yazma sahnesi; `target` verilirse o oturum (ya da yeni görev) seçili açılır. */
  function write(target: string | 'new' | null = null) {
    if (!writeOn) return;
    data.writeTarget = target;
    send({ type: 'write' });
  }
  const back = () => send({ type: 'back' });
  // Kaynaklar düzeni: ızgaradan Mail/GitHub ayrıntısı; geri dönünce mini Kaynaklar.
  function openSource(id: SourceId) {
    if (id === 'mail') return openMail();
    hub.detail = id;
    open();
  }
  const closeSource = () => send({ type: 'shrink' });
  // Saat ekranındaki Mail kartı ve Kaynaklar'daki Mail çipi: Mail sekmesi (Faz 5b).
  function openMail() {
    pick('mail');
  }
  $effect(() => {
    if ((island.level < 2 || island.tab !== 'ai') && untrack(() => hub.detail)) hub.detail = null;
  });
  // Yerel Git: GitHub kutucuğunun sayısı için açılışta (ya da özellik açılınca) depolara bir kez bakılır.
  // Sonrası yalnız istenince (Depolarım, Yenile, commit); yoklama yok.
  $effect(() => {
    if (localGitOn() && hubOn() && untrack(() => !git.overview && !git.loading && !git.error)) void loadGit();
  });
  // Raftan "Claude'a sor" (Yeni görünüm + Özellikler → Raftan Claude'a sor): sahne o dosya ekli açılır.
  const askOn = $derived(writeOn && features.shelf && features.shelfAsk);
  function ask(item: ShelfItem) {
    data.writeAttach = [{ path: item.path, name: item.name, dir: item.dir }];
    write();
  }
  // Bildirime tıklama: ilgili bölüm açılır. Biten bir oturumun bildirimi o oturumu seçer; yazıda dosya
  // sayısı varsa doğrudan ayrıntıya, o oturumun Değişenler sekmesine gidilir.
  function openNotice(n: NoticeData) {
    if (n.open) {
      data.selected = n.open.session;
      data.usageOpen = false;
      data.detailTab = n.open.section;
    }
    pick(n.tab);
    if (n.open?.section) open();
  }
  // Çentiğe tıklama: olay varsa onun bölümü; süren etkinlikte ilgili sekme (AI: o oturumun ayrıntısı).
  function openNotch(target: NotchTarget = 'notice') {
    peekKey = null;
    if (target === 'idle') return pointer('hover');
    if (target === 'notice') return notch.notice ? openNotice(notch.notice) : undefined;
    openItem(target);
  }
  // Fare çentikteki bir hapa geldi: onay/soru ve boştaki Zody ilgili bölümü açar; süren etkinlik ve bitti
  // gibi bilgi olayları ara boyda genişler (ayara göre doğrudan da açılabilir).
  // Mini pencerenin kapanma sayacı ayrıdır: adadan çıkışta hoverTimer temizlenir, bu temizlenmemeli.
  let miniTimer: ReturnType<typeof setTimeout> | undefined;
  /** Bekleme süresi dolunca açılacak hap (çıkış olayı yanlış geldiyse beklemeyi sürdürmek için). */
  let notchPending: NotchTarget | null = null;
  function notchHover(target: NotchTarget | null) {
    clearTimeout(hoverTimer);
    clearTimeout(miniTimer);
    const pending = notchPending;
    notchPending = null;
    if (target === null) {
      miniShut = null;
      // Hap açılmayı beklerken gelen "çıktı" olayı da doğrulanır; imleç hâlâ hapın üstündeyse açılış sürer.
      if (!peekKey && pending && inTauri) {
        void call<boolean>('island_pointer_inside')
          .catch(() => false)
          .then(inside => {
            if (inside && !peekKey && island.level === 0) notchHover(pending);
          });
        return;
      }
      // Açık hap (ara boy/mini) yalnız imleç gerçekten adanın alanından çıkınca kapanır. Ekranın en üst
      // satırından girilince WebView2 ve şerit penceresi (shelf/catcher.rs) imleç hâlâ hapın içindeyken
      // "çıktı" olayı üretebiliyor: kapatmadan önce Rust'a imlecin gerçek yeri sorulur.
      if (peekKey && inTauri) return leaveNotchWhenOut(miniKind && !reduce ? MINI_CLOSE_MS : 0);
      // Mini pencere fare kısa bir an dışarı kaçınca titremesin: MINI_CLOSE_MS bekleyip hap hâline döner.
      if (miniKind && !reduce) return void (miniTimer = setTimeout(leaveNotch, MINI_CLOSE_MS));
      return leaveNotch();
    }
    const key = targetKey(target);
    if (peekKey === key || island.level > 0) return;
    // "Kapat"la kapatılan mini pencere fare çentikten çıkana kadar yeniden açılmaz.
    if (miniShut === key) return;
    if (data.settings.island.reveal === 'click' || notchPrefs.hover === 'click') return;
    if (target === 'notice' && notch.notice?.finish) return;
    const delay = data.settings.island.hoverDelayMs;
    const widen = notchPrefs.hover === 'peek' || notchPrefs.hover === 'mini';
    if (miniFor(target, notchPrefs, notch.notice) || (widen && peekable(target, notch.notice))) {
      // Bir hap zaten açıksa diğerine geçiş beklemeden olur.
      if (peekKey) peekKey = key;
      else {
        notchPending = target;
        hoverTimer = setTimeout(() => {
          notchPending = null;
          peekKey = key;
        }, delay);
      }
      return;
    }
    notchPending = target;
    hoverTimer = setTimeout(() => {
      notchPending = null;
      if (typeof target === 'string') openNotch(target);
      else openItem(target);
    }, delay);
  }
  // Güvenlik ağı: çentikten çıkış olayı bazen ulaşmaz (pencere dışına hızlı çıkış). Açık mini pencerede fare
  // MINI_IDLE_MS boyunca hiç oynamadıysa (pencere dışındaki fare olay üretmez) hap hâline döner.
  let lastMove = Date.now();
  $effect(() => {
    if (!miniKind) return;
    lastMove = Date.now();
    let timer: ReturnType<typeof setTimeout>;
    let stopped = false;
    const check = () => {
      const left = MINI_IDLE_MS - (Date.now() - lastMove);
      if (left <= 0) {
        // İmleç hâlâ adanın üstünde kıpırdamadan duruyorsa (en üst satırda WebView2 olay almayabilir) açık kalır.
        if (inTauri) {
          void call<boolean>('island_pointer_inside')
            .catch(() => false)
            .then(inside => {
              if (stopped) return;
              if (inside) {
                lastMove = Date.now();
                timer = setTimeout(check, MINI_IDLE_MS);
              } else untrack(leaveNotch);
            });
          return;
        }
        return void untrack(leaveNotch);
      }
      timer = setTimeout(check, left);
    };
    timer = setTimeout(check, MINI_IDLE_MS);
    const move = () => (lastMove = Date.now());
    window.addEventListener('pointermove', move, { passive: true });
    return () => {
      stopped = true;
      clearTimeout(timer);
      window.removeEventListener('pointermove', move);
    };
  });
  /** İmleç adanın alanındaysa 150 ms'de bir yeniden bakar (yalnız hap açıkken); çıkınca hap hâline döner.
   * Hapa yeniden gelinince notchHover bu sayacı temizler. */
  function leaveNotchWhenOut(wait: number) {
    miniTimer = setTimeout(async () => {
      if (!peekKey) return;
      if (await call<boolean>('island_pointer_inside').catch(() => false)) {
        if (peekKey) leaveNotchWhenOut(150);
        return;
      }
      if (peekKey) leaveNotch();
    }, wait);
  }
  function leaveNotch() {
    peekKey = null;
    if (settleOnLeave && notice) {
      settleOnLeave = false;
      settle(notice.key);
    }
  }
  let miniShut: string | null = null;
  // Mini pencerelerin düğmeleri (NotchMini): mevcut AI, medya ve Zodymetre işlemleri.
  function miniAction(action: MiniAction) {
    switch (action.op) {
      case 'stop':
        return void call('task_stop', { session: action.session });
      case 'front':
        return void focusSession(action.session);
      case 'detail':
        peekKey = null;
        data.selected = action.session;
        data.usageOpen = false;
        data.detailTab = null;
        pick('ai');
        return open();
      case 'write':
        if (!writeOn) return miniAction({ op: 'detail', session: action.session });
        peekKey = null;
        return write(action.session);
      case 'close':
        // Bildirimse kalkar; süren etkinlik hap hâline döner.
        if (peekKey === 'notice' && notice && flash?.key === notice.key) flash = null;
        miniShut = peekKey;
        peekKey = null;
        return;
      case 'music':
        return musicControl(action.action);
      case 'timer': {
        const id = action.id;
        const op = action.action;
        if (op === 'reset') peekKey = null;
        return void todayApply(
          op === 'add' ? { op: 'timerAdd', id, minutes: 5 } : { op: op === 'start' ? 'timerStart' : op === 'pause' ? 'timerPause' : 'timerReset', id },
        );
      }
    }
  }
  // Süren etkinliğin sekmesi: AI → o oturumun ayrıntısı, müzik → müzik, Zodymetre → Bugün.
  function openItem(item: LiveItem) {
    peekKey = null;
    if (item.session) {
      data.selected = item.session;
      data.usageOpen = false;
      data.detailTab = null;
    }
    pick(item.tab);
    if (item.kind === 'working') open();
  }
  // Ara boydaki Zodymetre düğmeleri ve Bitiş bildirimi.
  function timerAction(op: 'start' | 'pause' | 'add', item: LiveItem) {
    if (item.kind === 'mode') {
      void todayApply(op === 'add' ? { op: 'modeAdd', minutes: 5 } : { op: op === 'start' ? 'modeResume' : 'modePause' });
      return;
    }
    const id = item.meterId;
    void todayApply(op === 'add' ? { op: 'timerAdd', id, minutes: 5 } : { op: op === 'start' ? 'timerStart' : 'timerPause', id });
  }
  function finishAction(action: FinishAction) {
    const fin = notch.notice?.finish;
    if (fin?.kind === 'mode') {
      if (action === 'reopen') void todayApply({ op: 'modeReopen' });
      else if (action === 'break') void startBreak();
      data.modeDone = null;
      return;
    }
    const id = fin?.kind === 'meter' ? (fin.meterId ?? undefined) : undefined;
    if (action === 'add') void todayApply({ op: 'timerAdd', id, minutes: 5 });
    else if (action === 'again') void todayApply({ op: 'timerStart', id });
    data.timerDone = null;
  }
  // Mod Bitişi › "Mola ver · 5 dk": "Mola" adlı Zodymetre (yoksa ve yer varsa eklenir) 5 dakikayla başlar.
  async function startBreak() {
    const find = () => data.today?.meters.find(m => m.name === 'Mola');
    if (!find()) {
      if ((data.today?.meters.length ?? 0) >= MAX_METERS) {
        toast = { key: `break:${Date.now()}`, mood: 'attention', title: 'Mola için yer yok', detail: `En çok ${MAX_METERS} Zodymetre olur`, tab: 'today' };
        clearTimeout(toastTimer);
        toastTimer = setTimeout(() => (toast = null), 4000);
        return;
      }
      const error = await todayApply({ op: 'meterAdd', name: 'Mola', minutes: 5, color: '#4fcf86', sound: true });
      if (error) return;
    }
    const meter = find();
    if (!meter) return;
    await todayApply({ op: 'timerSet', id: meter.id, minutes: 5 });
    await todayApply({ op: 'timerStart', id: meter.id });
  }

  function scheduleLeave() {
    clearTimeout(leaveTimer);
    // Raftan dışarı sürüklerken imleç adadan çıkar; sürükleme bitene dek ada açık kalır. Zody'ye yazarken
    // (ya da cevap beklerken) de kapanmaz: imleç kayınca yazı yarıda kalmasın; Esc ya da dışarı tıklama kapatır.
    const leaving = () => !pointerInside && !data.shelfOut && !(tab === 'zody' && chatHolds());
    leaveTimer = setTimeout(async () => {
      if (!leaving()) return;
      // Ada büyürken imleç henüz küçük olan öğeden çıkıp büyüyen alanda durursa "girdi" olayı gelmez:
      // kapatmadan önce imlecin gerçek yeri sorulur. Hâlâ adadaysa yeniden bakılır (ilk kıpırtıda
      // onmousemove içeri sayar ve bu bekleme biter).
      if (inTauri && island.level > 0 && (await call<boolean>('island_pointer_inside'))) {
        if (leaving()) scheduleLeave();
        return;
      }
      if (leaving()) send({ type: 'leave' });
    }, leaveDelay(island.level));
  }

  // Dosya rafı: adaya dosya sürüklenince raf açılır (OLE sürüklemesi sürerken fare olayı gelmez, adayı
  // bu olay açar). Bırakınca kısa bir not kalır; fare adaya girmezse ada kapanır. Vazgeçilirse hemen kapanır.
  const DROP_STAY_MS = 1800;
  let dropped = $state<number | null>(null);
  let droppedTimer: ReturnType<typeof setTimeout> | undefined;
  function shelfDragged(drag: ShelfDrag) {
    clearTimeout(leaveTimer);
    clearTimeout(hoverTimer);
    notchPending = null;
    pointerInside = false;
    if (drag.kind === 'enter') {
      greeter.cancel();
      send({ type: 'tab', tab: 'shelf' });
    } else if (drag.kind === 'send') {
      // Bir AI oturumunun adına bırakıldı: sonucun notu (shelf://sent) okunsun diye ada biraz açık kalır.
      leaveTimer = setTimeout(() => {
        if (!pointerInside) send({ type: 'leave' });
      }, DROP_STAY_MS + 1200);
    } else if (drag.kind === 'drop') {
      dropped = drag.count;
      clearTimeout(droppedTimer);
      droppedTimer = setTimeout(() => (dropped = null), DROP_STAY_MS + 1200);
      leaveTimer = setTimeout(() => {
        if (!pointerInside) send({ type: 'leave' });
      }, DROP_STAY_MS);
    } else if (island.tab === 'shelf') {
      send({ type: 'leave' });
    }
  }
  function shelfOutDone() {
    pointerInside = false;
    scheduleLeave();
  }

  function pointer(kind: string) {
    if (kind === 'hover') {
      pointerInside = true;
      send({ type: 'hover', tab: notch.notice?.tab ?? notch.lead?.tab ?? openTab });
    } else if (kind === 'enter') {
      pointerInside = true;
      clearTimeout(leaveTimer);
    } else if (kind === 'leave') {
      pointerInside = false;
      scheduleLeave();
    } else if (kind === 'outside') {
      greeter.cancel();
      send({ type: 'outside' });
    } else if (kind === 'escape') {
      greeter.cancel();
      send({ type: 'escape' });
    } else if (kind === 'stripEnter') {
      // Rust'taki bırakma penceresi şeride girişi gördü (ekranın en üst satırında WebView2 fare olayı
      // almayabilir): DOM'un kendi girişi gelmediyse aynısı yapılır.
      if (island.level === 0 && !stripHover) onmouseenter();
    } else if (kind === 'stripLeave') {
      if (island.level === 0 && stripHover) onmouseleave();
    }
  }

  // Animasyon sürerken eski ve yeni alanın birleşimi (büyürken yayın taşma payıyla), bitince yalnız
  // yeni alan: ikisi tek çağrıda gider, son alana geçişi Rust zamanında yapar. Arayüzün zamanlayıcısı
  // kısılırsa (ör. odak Ayarlar penceresindeyken) bölge büyük kalıp adanın yanında boş bir şerit
  // bırakmasın. Rust bu alanın dışındaki tıklamaları alttaki uygulamaya geçirir.
  let reported: Size = NOTCH;
  let sent = '';
  function report(level: number, during: Size, s: Size, ms: number) {
    const width = window.innerWidth || 740;
    const rect = (r: Size) => ({ x: Math.max(0, (width - r.w) / 2), y: 0, w: r.w, h: r.h });
    const same = during.w === s.w && during.h === s.h;
    const area = { level, ...rect(during), settle: same ? null : { ...rect(s), ms } };
    const key = JSON.stringify(area);
    if (key === sent) return;
    sent = key;
    void call('island_update', area);
  }
  $effect(() => {
    const s = size;
    const level = island.level;
    const duration = morph;
    untrack(() => {
      report(level, duration ? morphArea(reported, s) : s, s, duration + 40);
      reported = s;
    });
  });

  // Ada bir üst seviyeye açılırken içerik, büyüme başladıktan iki kare sonra kurulur: bileşenlerin ilk
  // çizimi animasyonun ilk karesini geciktirmez. Kapanırken ve bölüm değişirken hemen güncellenir.
  let shown = $state<{ level: number; tab: Tab; write?: boolean }>({ level: 0, tab: 'ai' });
  let mountFrame = 0;
  $effect(() => {
    const next = { level: island.level, tab, write: island.write };
    untrack(() => {
      cancelAnimationFrame(mountFrame);
      if (next.level <= shown.level || reduce) shown = next;
      else mountFrame = requestAnimationFrame(() => (mountFrame = requestAnimationFrame(() => (shown = next))));
    });
  });

  // Isıtma: açılıştan kısa süre sonra (boşta) her bölümün mini ve ayrıntı görünümü bir kez görünmeden
  // çizilip kaldırılır. İlk açılıştaki uzun kare (bileşen kodunun ilk derlenmesi, ilk stil hesabı)
  // böylece açılış anına düşmez.
  // Önce yalnız görünen sekmelerin parçaları yüklenir (kapalı modülünkiler hiç); sabit sekmenin tek
  // ekranı, AI'nın minisi ve ayrıntısı.
  let warm = $state<{ level: 1 | 2; tab: Tab }[]>([]);
  async function prewarm() {
    const tabs = ctx.tabs;
    await Promise.all(tabs.flatMap(tab => TAB_VIEWS[tab].map(load)));
    warm = tabs.flatMap(tab => (isFixed(tab) ? [{ level: 2 as const, tab }] : [{ level: 1 as const, tab }, { level: 2 as const, tab }]));
    requestAnimationFrame(() => requestAnimationFrame(() => (warm = [])));
  }

  // Sistem sayaçları yalnız Bilgisayar bölümü (ya da Saat'teki Bilgisayar kartı) ekrandayken okunur: minide göstergeler,
  // ayrıntıda uygulama listesiyle birlikte. Başka her durumda hiç ölçüm yapılmaz.
  // Saat ekranında Bilgisayar kartı seçiliyse yalnız CPU/RAM (seviye 1) okunur.
  const clockPc = $derived(tab === 'clock' && island.level === 2 && data.settings.clock.cards.includes('pc'));
  const systemWatch = $derived<0 | 1 | 2>(!data.settings.modules.system || island.level === 0 ? 0 : tab === 'pc' ? (island.level === 2 ? 2 : 1) : clockPc ? 1 : 0);
  $effect(() => setSystemWatch(systemWatch));

  // Uyku: ada açılınca ya da ajan/müzik/onay durumu değişince sayaç baştan başlar. Anahtar bir dize
  // olduğundan Zody'nin kendi uykuya geçmesi (ctx.mood) bu etkiyi tetiklemez.
  const activityKey = $derived(
    `${island.level}|${ctx.approvals.length}|${ctx.music?.playing ?? '-'}|${ctx.agents.map(a => a.status).join()}`,
  );
  $effect(() => {
    void activityKey;
    untrack(noteActivity);
  });

  // Yeni bir onay isteği: ada kapalıysa çentik bildirimi görünür (adayı açmaz), açıksa AI bölümüne geçilir.
  // Sesi Rust çalar (ayarlar ve Windows "Rahatsız etme" orada denetlenir).
  let seen = new Set<string>();
  $effect(() => {
    // Katlanan kartlar da sayılır: "göster" ile geri gelince yeniden ses çalmasın.
    const ids = [...ctx.approvals, ...ctx.folded].map(r => r.id);
    untrack(() => {
      const fresh = ids.some(id => !seen.has(id));
      seen = new Set(ids);
      if (!fresh) return;
      send({ type: 'attention' });
      void call('sound_play', { kind: 'attention' });
    });
  });

  // Dolan zamanlayıcının bildirimi Bugün, gelen planın bildirimi Ajanda görülünce kalkar.
  $effect(() => {
    if (island.level === 0) return;
    if (tab === 'today') {
      if (data.timerDone?.meter) data.timerDone = null;
    } else if (tab === 'agenda') {
      if (data.eventDue) data.eventDue = null;
    } else if (tab === 'modes') {
      if (data.modeDone) data.modeDone = null;
      if (data.timerDone?.focus) data.timerDone = null;
    }
  });

  // Bir ajan işini bitirdi ya da hata verdi: ses (ayar açıksa), ada kapalıysa kısa bildirim (açıksa
  // kullanıcı zaten görüyor).
  let statuses = new Map<string, AgentStatus>();
  let flashTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const agents = ctx.agents;
    void agents.map(a => a.status).join();
    untrack(() => {
      const events = agentEvents(statuses, agents, Date.now());
      statuses = new Map(agents.map(a => [a.id, a.status]));
      const last = events.at(-1);
      if (last) void call('sound_play', { kind: last.kind });
      if (!last) return;
      // Ölçüm (Faz 7.2; yalnız tanı kaydı açıkken yazılır): Zodyvel'in olayı aldığı andan çentiğin çizildiği ana.
      const at = agents.find(a => a.id === last.session)?.turnEndedAt ?? undefined;
      const skip = island.level > 0 ? 'ada_acik' : !data.settings.island.flash ? 'kapali' : null;
      if (skip) return void call('report_mark', { label: `cizim:${last.kind}:${skip}`, at });
      flash = last;
      requestAnimationFrame(() => void call('report_mark', { label: `cizim:${last.kind}`, at }));
      clearTimeout(flashTimer);
      flashTimer = setTimeout(() => (flash = null), FLASH_MS);
    });
  });

  // Bekleme hatırlatması (Özellikler → açıksa): yanıtını uzun süredir bekleyen oturum bir kez hatırlatılır.
  // Ayrı zamanlayıcı yok: ajan verisinin dakikalık hesabıyla (data.minute) birlikte bakılır. Uzun bekleyen
  // onay isteğinde başlık değişir (noticeFor); eşik aşıldığı an bir kez ses çalar.
  const REMIND_MS = 8000;
  let reminded = new Set<string>();
  let reminderTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const on = features.waitAlert;
    const minutes = features.waitAlertMin;
    const agents = ctx.agents;
    void data.minute;
    untrack(() => {
      if (!on) return;
      const due = dueReminder(agents, Date.now(), minutes, reminded);
      if (!due) return;
      reminded.add(due.key);
      if (island.level > 0) return;
      reminder = due;
      void call('sound_play', { kind: 'attention' });
      clearTimeout(reminderTimer);
      reminderTimer = setTimeout(() => (reminder = null), REMIND_MS);
    });
  });
  let lateSeen = new Set<string>();
  $effect(() => {
    const key = notice?.key;
    if (!key?.endsWith(':late')) return;
    untrack(() => {
      if (lateSeen.has(key)) return;
      lateSeen.add(key);
      void call('sound_play', { kind: 'attention' });
    });
  });

  // Arka planda başlatılan görev hatayla bitti: ada açıksa yazma sahnesi gösterir, kapalıysa çentik notu.
  function taskFailed(fail: TaskFailed) {
    if (island.level > 0) return;
    toast = {
      key: `task:${fail.sessionId}`,
      mood: 'error',
      title: fail.taskNo ? `${fail.taskNo}. görev başlamadı` : 'Görev başlamadı',
      detail: fail.message,
      tab: 'ai',
    };
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), REMIND_MS);
  }

  // Pil notu (Özellikler → Pil bildirimleri): şarja takıldı/çıkarıldı, pil azaldı. Ada açıksa gösterilmez.
  function batteryChanged(before: Battery | null, after: Battery | null) {
    const note = data.settings.features.batteryAlerts ? batteryNote(before, after, Date.now()) : null;
    if (!note || island.level > 0) return;
    toast = note;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), note.mood === 'attention' || note.mood === 'error' ? REMIND_MS : 4000);
  }

  // Pazartesi özeti (Özellikler → Pazartesi haftalık özet): haftanın ilk açılışında bir kez geçen hafta.
  // Hafta değişimi dakikalık sayaçla fark edilir; gösterilen hafta bu pencerenin deposunda tutulur.
  const RECAP_KEY = 'zodyvel.weeklyRecap';
  const recapWeek = $derived(weekKey(new Date(data.minute)));
  $effect(() => {
    const week = recapWeek;
    if (!features.usage || !features.weeklyRecap) return;
    untrack(() => void showRecap(week));
  });
  async function showRecap(week: string) {
    let shown: string | null = null;
    try {
      shown = localStorage.getItem(RECAP_KEY);
    } catch {
      return; // Depo yoksa her açılışta göstermektense hiç göstermez.
    }
    if (shown === week) return;
    await loadUsage();
    // Ada o an açıksa bu hafta için bir sonraki açılışa kalır.
    if (island.level > 0) return;
    try {
      localStorage.setItem(RECAP_KEY, week);
    } catch {
      return;
    }
    const note = data.usage ? weeklyNote(data.usage.summary, data.usage.spans) : null;
    if (!note) return;
    toast = note;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), REMIND_MS);
  }

  // Onay kısayolları yalnız karar verilebilir bir kart ekrandayken etkin: Rust o sürece tuşları kaydeder.
  let visible = $state(true);
  const keyTarget = $derived.by(() => {
    const first = ctx.approvals[0];
    return visible && island.level >= 1 && tab === 'ai' && !island.write && first?.actionable && first.kind === 'permission' ? first.id : null;
  });
  $effect(() => {
    const target = keyTarget;
    data.keyTarget = target;
    void call('shortcuts_arm', { target });
  });

  // Onaya git / Terminale git kısayolları (Yeni görünüm + Özellikler → Onaya ve terminale git).
  const jumpOn = $derived(features.jumpKeys && data.settings.modules.agents);
  function note(title: string, detail: string) {
    toast = { key: `jump:${Date.now()}`, mood: 'idle', title, detail, tab: 'ai' };
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), TOAST_MS);
  }
  function goApproval() {
    const jump = approvalJump(ctx.approvals, ctx.folded);
    if (!jump) return note('Bekleyen onay yok', 'Bir istek gelince bu kısayol onu açar');
    // Katlanan kart "göster" gibi geri açılır; istenen istek öne alınır.
    if (jump.unfold) unfoldApprovals();
    data.approvalFirst = jump.id;
    send({ type: 'tab', tab: 'ai' });
  }
  async function goTerminal() {
    const target = terminalTarget(ctx.agents, [...ctx.approvals, ...ctx.folded], data.selected);
    if (!target) return note('Öne getirilecek pencere yok', 'Penceresi bilinen açık oturum bulunamadı');
    const error = await focusSession(target.id);
    if (error) note(`${target.project}: pencere açılamadı`, error);
  }

  function shortcut(action: string, id: string | null) {
    if (action === 'toggle') {
      if (visible) send({ type: 'toggle', tab: openTab });
    } else if (action === 'write') {
      if (visible) write();
    } else if (action === 'approval') {
      if (visible && jumpOn) goApproval();
    } else if (action === 'terminal') {
      // Ada gizliyken de çalışır (tam ekran uygulamadan terminale dönmek için).
      if (jumpOn) void goTerminal();
    } else if ((action === 'approve' || action === 'deny') && id && id === data.keyTarget) {
      data.keyDecision = { id, decision: action === 'approve' ? 'allow' : 'deny' };
    }
  }

  // Sustur kısayolu / tepsi: kısa "Sesler kapalı/açık" notu.
  const TOAST_MS = 2500;
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  function soundToggled(on: boolean) {
    const key = data.settings.shortcuts.mute;
    toast = {
      key: `sound:${Date.now()}`,
      mood: on ? 'idle' : 'sleep',
      title: on ? 'Sesler açık' : 'Sesler kapalı',
      detail: key ? `${key} ile değiştir` : 'Tepsiden değiştir',
      tab: ctx.hoverTab,
    };
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), TOAST_MS);
  }

  // Güncelleme sonrası ilk açılışta "0.x.y'ye güncellendi" notu (bir kez); sürüm ayarlara kaydedilir.
  async function checkUpdated() {
    if (!inTauri) return;
    try {
      const current = await getVersion();
      const previous = data.settings.island.lastSeenVersion;
      if (previous === current) return;
      const note = updateNote(previous, current, Date.now());
      const next = structuredClone($state.snapshot(data.settings));
      next.island.lastSeenVersion = current;
      await saveSettings(next);
      if (note) {
        toast = note;
        clearTimeout(toastTimer);
        toastTimer = setTimeout(() => (toast = null), REMIND_MS);
      }
    } catch {
      // Sürüm okunamazsa not gösterilmez.
    }
  }

  // Pencere bölgesi ada şeklinde olduğu için fare olayları yalnız ada üzerindeyken gelir.
  // Şeritte kısa bekleme (sekmelere giderken yanlışlıkla açılmasın) sonra mini.
  let hoverTimer: ReturnType<typeof setTimeout> | undefined;
  /** İmleç kapalı adanın şeridinde (DOM girişi ya da Rust'un "stripEnter" bildirimiyle); ikisi bir kez sayılır. */
  let stripHover = false;
  $effect(() => {
    // Ada açıldı: şerit girişi tüketildi; kapanınca bir sonraki giriş yeniden sayılır.
    if (island.level > 0) stripHover = false;
  });
  function onmouseenter() {
    if (island.level === 0) {
      if (stripHover) return;
      stripHover = true;
      // Hapta dinlenirken WebView2 önbelleklerini bırakmış olabilir: bekleme süresinde geri istenir.
      void call('island_prewarm');
      clearTimeout(hoverTimer);
      const delay = data.settings.island.hoverDelayMs;
      if (greeter.enter(delay, notchOn)) return;
      // Tıklama kipinde yalnız tıklamayla açılır (çentiğe ya da Zody'ye).
      if (data.settings.island.reveal === 'click') return;
      // Çentikte hap varsa fareyi haplar karşılar (notchHover); Bitiş zaten büyük ve düğmeli.
      if (notchOn) return;
      hoverTimer = setTimeout(() => pointer('hover'), delay);
    } else {
      pointer('enter');
    }
  }
  function onmouseleave() {
    stripHover = false;
    notchHover(null);
    clearTimeout(hoverTimer);
    greeter.cancel();
    pointer('leave');
  }

  // Fare ada üzerinde uzun süre kıpırdamazsa sürekli animasyonlar duraklar; ilk harekette devam eder.
  const REST_MS = 12_000;
  let resting = $state(false);
  let restTimer: ReturnType<typeof setTimeout> | undefined;
  function onmousemove() {
    // Sürükle-bırak sonrası tarayıcı "girdi" olayını atlayabilir: fare adada kımıldıyorsa içeride sayılır.
    if (!pointerInside && island.level > 0) pointer('enter');
    if (resting) resting = false;
    clearTimeout(restTimer);
    restTimer = setTimeout(() => (resting = true), REST_MS);
  }

  onMount(() => {
    void loadSettings().then(() => {
      greeter.start();
      void checkUpdated();
    });
    const stops = [
      watchSettings(),
      watchAgents(),
      watchMusic(),
      watchSystem(),
      watchToday(),
      watchSources(),
      watchShelf(shelfDragged),
      watchPower(batteryChanged),
      // Ada odak almadığından klavye ve dışarı tıklama olaylarını Rust iletir.
      on<{ kind: string }>('island://pointer', p => pointer(p.kind)),
      on<{ visible: boolean }>('island://visibility', p => {
        visible = p.visible;
        if (!p.visible) send({ type: 'hide' });
      }),
      on<{ action: string; id: string | null }>('island://shortcut', p => shortcut(p.action, p.id)),
      on<boolean>('island://sound', soundToggled),
      on<VolumeOsd>('volume://osd', volumeOsd),
      watchTasks(taskFailed),
      watchChat(),
    ];
    if (!inTauri) stops.push(browserPreview());
    const warmUp = () => void prewarm();
    const idle = window.requestIdleCallback?.(warmUp, { timeout: 4000 }) ?? setTimeout(warmUp, 1500);
    return () => {
      stops.forEach(stop => stop());
      if (window.cancelIdleCallback) window.cancelIdleCallback(idle);
      else clearTimeout(idle);
    };
  });

  // Tarayıcı önizlemesi (pnpm dev): dışarı tıklama ve Esc DOM'dan gelir.
  function browserPreview(): () => void {
    const down = (e: PointerEvent) => {
      if (!(e.target as Element).closest('.island')) pointer('outside');
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') pointer('escape');
      // Varsayılan kısayolların önizlemesi (gerçek uygulamada Rust kaydeder).
      const action = e.ctrlKey && e.altKey ? ({ z: 'toggle', y: 'write', o: 'approval', t: 'terminal', enter: 'approve', backspace: 'deny' } as Record<string, string>)[e.key.toLowerCase()] : undefined;
      if (action) shortcut(action, action === 'approve' || action === 'deny' ? data.keyTarget : null);
    };
    document.addEventListener('pointerdown', down);
    document.addEventListener('keydown', key);
    return () => {
      document.removeEventListener('pointerdown', down);
      document.removeEventListener('keydown', key);
    };
  }
</script>

{#snippet miniPane(target: LiveItem | 'notice')}
  {@const sessionId = target === 'notice' ? notch.notice?.open?.session : target.session}
  <NotchMini
    kind={miniKind ?? 'ai'}
    session={sessionId ? (ctx.agents.find(a => a.id === sessionId) ?? null) : null}
    music={ctx.music}
    art={data.artUrl}
    meters={ctx.meters}
    item={target === 'notice' ? null : target}
    now={Math.max(liveNow, data.minute)}
    {reduce}
    onaction={miniAction}
  />
{/snippet}

{#snippet view(level: number, t: Tab, scene = false)}
  {#if scene}
    <!-- Yazma sahnesi simge çubuğunun ve mini AI'nın yerini alır. -->
    {@const Write = lazy('write')}
    {#if Write}<Write {ctx} onback={back} />{/if}
  {:else}
    <IconBar {ctx} tab={t} detail={level === 2} ontab={pick} onopen={open} onwrite={writeOn ? () => write() : undefined} onlaunch={() => send({ type: 'outside' })} />
    {#if t === 'ai'}
      {#if level === 1}
        {@const Mini = lazy('miniAI')}
        {#if Mini}<Mini {ctx} onopen={open} onmusic={() => pick('music')} onwrite={writeOn ? write : undefined} onsource={openSource} />{/if}
      {:else if hub.detail && hubOn()}
        {@const Source = lazy('github')}
        {#if Source}<Source state={sourceState(hub.detail)} onback={closeSource} />{/if}
      {:else}
        {@const Detail = lazy('detailAI')}
        {#if Detail}<Detail {ctx} onwrite={writeOn ? write : undefined} />{/if}
      {/if}
    {:else}
      <!-- K11: sabit sekmeler tek ekrandır; hepsi ayrıntı kipinde çizilir. -->
      {@const Screen = lazy(t)}
      {#if Screen}
        {#if t === 'music' || t === 'today' || t === 'modes' || t === 'agenda'}
          <Screen {ctx} />
        {:else if t === 'mail'}
          <Screen state={sourceState('mail')} />
        {:else if t === 'clock'}
          <Screen {ctx} ontab={pick} onmail={hubOn() && data.settings.features.sourceMail ? openMail : undefined} />
        {:else if t === 'pc'}
          <Screen system={ctx.system} battery={ctx.battery} detail onopen={open} />
        {:else if t === 'zody'}
          <Screen detail />
        {:else if t === 'shelf'}
          <Screen {ctx} detail {dropped} ondone={shelfOutDone} onask={askOn ? ask : undefined} />
        {/if}
      {/if}
    {/if}
  {/if}
{/snippet}

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="island"
  class:grow
  class:reduce-motion={reduce}
  class:resting={resting && island.level > 0}
  class:lit={aura !== null}
  class:bare={island.level === 0 && notchOn && (osdOn || !notch.notice?.finish) && !greetSize}
  style:--ix="{inset.x}px"
  style:--iy="{inset.y}px"
  style:--r="{radius}px"
  style:--edge="{edge}px"
  style:--morph="{morph}ms"
  {onmouseenter}
  {onmouseleave}
  {onmousemove}
>
  <div class="fill"></div>
  {#if aura}
    {#key aura.key}
      <div class="aura-box" style:height="{size.h}px"><Aura color={aura.color} strength={aura.strength} live={aura.live} /></div>
    {/key}
  {/if}
  {#if tour}
    <div class="content enter" style:width="{size.w}px" style:height="{size.h}px">
      <Welcome onclose={() => greeter.finishTour()} />
    </div>
  {:else if shown.level > 0}
    {@const s = sizeFor(shown.level as 1 | 2, shown.tab)}
    {#key `${shown.level}-${shown.tab}-${shown.write ?? ''}`}
      <!-- Boş yere tıklama kısayolu yalnız fare için; klavye ayrıntıya kartlardan gider. -->
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="content enter" style:width="{s.w}px" style:height="{s.h}px" out:fade={{ duration: reduce ? 0 : 70 }} onclick={shown.level === 1 && !shown.write ? onminiclick : undefined}>
        {@render view(shown.level, shown.tab, !!shown.write)}
      </div>
    {/key}
  {:else if notchOn && island.level === 0}
    {#key osdOn ? 'osd' : (notch.notice?.key ?? 'live')}
      <div class="content" style:width="{size.w}px" style:height="{size.h}px" out:fade={{ duration: reduce ? 0 : 70 }}>
        {#if osdOn && osd}
          <VolumeOsdView {osd} />
        {:else if notch.notice?.finish}
          <Finish notice={notch.notice} {reduce} onopen={() => openNotch()} onaction={finishAction} />
        {:else}
          <Notch
            view={notch}
            {fresh}
            {peekKey}
            {reduce}
            art={data.artUrl}
            bind:width={notchW}
            onopen={openNotch}
            onhover={notchHover}
            ontimer={timerAction}
            onmusic={musicControl}
            mini={miniKind}
            miniH={notchSize?.h ?? 0}
            minipane={miniPane}
          />
        {/if}
      </div>
    {/key}
  {:else if greetSize && island.level === 0}
    <div class="content" style:width="{size.w}px" style:height="{size.h}px" out:fade={{ duration: reduce ? 0 : 70 }}>
      <Peek wave={greeter.greeting === 'hello' || !!greeter.peek?.wave} onopen={() => greeter.click()} />
    </div>
  {/if}
  {#if island.level === 0}
    {#each warm as w (`${w.level}-${w.tab}`)}
      <div class="content warm" style:width="{sizeFor(w.level, w.tab).w}px" style:height="{sizeFor(w.level, w.tab).h}px" aria-hidden="true" inert>
        {@render view(w.level, w.tab)}
      </div>
    {/each}
  {/if}
</div>

<style>
  /* Ada her zaman pencere boyutundadır (levels.ts WINDOW); görünen şekil clip-path ile kırpılır.
     Dış katman kenar çizgisinin rengindedir, .fill onu 1 px içeriden ada rengiyle örter. */
  .island {
    position: absolute;
    top: 0;
    left: 50%;
    width: 740px;
    height: 460px;
    transform: translateX(-50%);
    background: color-mix(in srgb, var(--island), rgb(255 255 255) 7%);
    clip-path: inset(0 var(--ix) var(--iy) var(--ix) round 0 0 var(--r) var(--r));
    transition: clip-path var(--morph) var(--shrink);
    contain: strict;
  }
  .fill {
    position: absolute;
    inset: 0;
    background: var(--island);
    clip-path: inset(
      0 calc(var(--ix) + var(--edge)) calc(var(--iy) + var(--edge)) calc(var(--ix) + var(--edge))
        round 0 0 calc(var(--r) - var(--edge)) calc(var(--r) - var(--edge))
    );
    transition: clip-path var(--morph) var(--shrink);
  }
  /* Canlı çentik: ada zemini saydam, hapları ve baloncukları Notch.svelte kendisi çizer (aralar boş). */
  .bare,
  .bare .fill {
    background: transparent;
  }
  .grow,
  .grow .fill {
    transition-timing-function: var(--spring-open);
  }
  .resting :global(*) {
    animation-play-state: paused !important;
  }
  /* Dalga varken cam kartlar (AI: .card, raf: .item) koyu bir zemin alır: dalga altından yine görünür
     ama yazılar okunur kalır. İç içe kart (ör. onay kartı) ikinci kez koyulaşmaz. */
  .island.lit :global(.card),
  .island.lit :global(.item) {
    background: var(--glass), rgb(10 10 12 / 0.72);
  }
  .island.lit :global(.card .card) {
    background: none;
  }
  .aura-box {
    position: absolute;
    z-index: 0;
    left: var(--ix);
    right: var(--ix);
    bottom: var(--iy);
    pointer-events: none;
  }
  .content {
    position: absolute;
    z-index: 1;
    top: 0;
    left: 50%;
    transform: translateX(-50%);
  }
  /* İçerik kademeli belirir: önce simge çubuğu, sonra görünüm. Yalnız opacity ve transform. */
  .enter > :global(*) {
    animation: rise var(--t-base) var(--ease) both;
  }
  .enter > :global(:nth-child(1)) {
    animation-delay: 60ms;
  }
  .enter > :global(:nth-child(n + 2)) {
    animation-delay: 120ms;
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(-8px) scale(0.985);
    }
  }
  .warm {
    opacity: 0;
    pointer-events: none;
  }
</style>
