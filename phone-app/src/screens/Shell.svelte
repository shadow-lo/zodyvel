<script lang="ts">
  // Eşli telefonun ana kabuğu: alt sekmeler (Oturumlar · Yeni görev · Ajanda · Ayarlar) ve oturum/onay/soru açılışı.
  // A-O1: yoklama yok. Canlı durum yalnız Rust'ın `live-changed` olayında ve ekran görünür olunca çekilir; üst üste
  // gelen istekler birleşir. `now` için tek zamanlayıcı: ayrıntı penceresi açıkken saniye başında (geri sayım), değilse
  // dakika başında; ekran gizliyken hiçbiri çalışmaz.
  import { onMount } from 'svelte';
  import Icon from '../ui/Icon.svelte';
  import Zody from '../Zody.svelte';
  import { ipc, onLiveChanged } from '../lib/ipc';
  import { errText } from '../lib/format';
  import { approvalFor } from '../lib/view';
  import type { AppState, LiveState } from '../lib/types';
  import Sessions from './Sessions.svelte';
  import SessionDetail from './SessionDetail.svelte';
  import Approval from './Approval.svelte';
  import Question from './Question.svelte';
  import NewTask from './NewTask.svelte';
  import Agenda from './Agenda.svelte';
  import Settings from './Settings.svelte';

  let { st, onchange }: { st: AppState; onchange: () => void } = $props();

  type Tab = 'sessions' | 'new' | 'agenda' | 'settings';
  let tab = $state<Tab>('sessions');
  let openId = $state<string | null>(null);
  let live = $state<LiveState | null>(null);
  let now = $state(Date.now());
  let err = $state('');
  let busy = $state(false);

  const visible = () => document.visibilityState === 'visible';

  // Çekim sürerken gelen istekler birleşir: biri bitince en çok bir kez daha çekilir. Dönen söz son çekimle biter.
  let fetching: Promise<void> | null = null;
  let dirty = false;
  function poll(): Promise<void> {
    dirty = true;
    fetching ??= (async () => {
      while (dirty) {
        dirty = false;
        try { live = await ipc.liveState(); now = Date.now(); } catch { /* sonraki olayda yeniden */ }
      }
      fetching = null;
    })();
    return fetching;
  }

  const until = $derived(live?.unlocked_until ?? null);
  let tick: ReturnType<typeof setTimeout> | undefined;
  /** Tek `now` zamanlayıcısı: pencere açıkken bir sonraki saniye, değilse bir sonraki dakika başı; gizliyken yok. */
  function schedule() {
    clearTimeout(tick);
    tick = undefined;
    if (!visible()) return;
    const t = Date.now();
    const step = until != null && until > t ? 1000 : 60_000;
    tick = setTimeout(() => {
      now = Date.now();
      if (until != null && now >= until) poll();
      schedule();
    }, step - (t % step));
  }
  $effect(() => {
    void until; // pencere açılıp kapanınca adım değişir
    schedule();
    return () => clearTimeout(tick);
  });

  onMount(() => {
    ipc.liveResume().catch(() => {}).finally(poll);
    let stop: (() => void) | null = null;
    let gone = false;
    onLiveChanged(() => { if (visible()) poll(); }).then((f) => { if (gone) f(); else stop = f; }).catch(() => {});
    const vis = () => { if (visible()) { now = Date.now(); poll(); } schedule(); };
    document.addEventListener('visibilitychange', vis);
    return () => { gone = true; stop?.(); clearTimeout(tick); document.removeEventListener('visibilitychange', vis); };
  });

  const unlocked = $derived(until != null && until > now);

  /** Parmak izi/PIN ile 2 dk ayrıntı penceresi açar. İptal edilirse hata metni gösterilir. */
  async function unlock(): Promise<boolean> {
    if (unlocked) return true;
    busy = true; err = '';
    try { await ipc.unlock(); await poll(); return true; } catch (e) { err = errText(e); return false; } finally { busy = false; }
  }
  async function openSession(id: string) {
    if (await unlock()) openId = id;
  }
  function go(t: Tab) { tab = t; openId = null; err = ''; }

  const connected = $derived(live?.link === 'connected');
  const session = $derived(openId ? live?.sessions.find((s) => s.id === openId) : undefined);
  const pending = $derived(session && live ? approvalFor(session, live.approvals) : undefined);
  // Açık oturum listeden düştüyse (bitti ve kapandı) listeye dönülür.
  $effect(() => { if (openId && live && connected && !session) openId = null; });
  const full = $derived(!!(session && pending)); // onay/soru ekranı tam ekran, sekmesiz
</script>

<div class="screen" class:amber={full}>
  {#if session && pending && live}
    {#if pending.kind === 'question'}
      <Question ap={pending} {session} {unlocked} {unlock} onback={() => (openId = null)} ondone={poll} />
    {:else}
      <Approval ap={pending} {session} {unlocked} {unlock} onback={() => (openId = null)} ondone={poll} />
    {/if}
  {:else if session && live}
    <SessionDetail {session} {live} {now} {unlocked} {unlock} onback={() => (openId = null)} ondone={poll} />
  {:else}
    <div class="ah">
      <Zody size="sm" tone={connected ? 'accent' : 'idle'} />
      <div class="tx"><b>{tab === 'sessions' ? 'Oturumlar' : tab === 'new' ? 'Yeni görev' : tab === 'agenda' ? 'Ajanda' : 'Ayarlar'}</b><small>{st.desktop_name ?? 'Eşli bilgisayar'}</small></div>
      <span class="net" class:off={!connected}><i></i>{connected ? 'Ev ağı' : 'Ağ dışı'}</span>
    </div>
    {#if err}<div class="banner err mx">{err}</div>{/if}
    {#if tab === 'sessions'}
      <Sessions {live} {now} {busy} onopen={openSession} />
    {:else if tab === 'new'}
      <NewTask {live} ondone={() => { go('sessions'); poll(); }} />
    {:else if tab === 'agenda'}
      <Agenda {live} {now} />
    {:else}
      <Settings {st} {live} {now} {unlocked} {unlock} {onchange} onrefresh={poll} />
    {/if}
  {/if}
  {#if !full}
    <nav class="tabs">
      {#each [['sessions', 'list', 'Oturumlar'], ['new', 'plus', 'Yeni görev'], ['agenda', 'cal', 'Ajanda'], ['settings', 'gear', 'Ayarlar']] as [k, ic, n] (k)}
        <button class:on={tab === k && !session} onclick={() => go(k as Tab)}><Icon name={ic ?? ''} size={20} />{n}</button>
      {/each}
    </nav>
  {/if}
</div>
