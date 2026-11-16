<script lang="ts">
  // Zody asistanı (Deneysel): bu bilgisayardaki resmi Claude Code, kendi girişinle. Zodyvel Claude'un kimlik
  // bilgilerini okumaz, saklamaz, taşımaz; durum yalnız `claude --version` ve küçük bir deneme isteğiyle anlaşılır.
  // Kapalıyken hiçbir süreç açılmaz ve bellek tutulmaz; durum da yoklanmaz.
  import { onMount } from 'svelte';
  import { installLine, loginLine, MODEL_LABELS, parseLogin, type BrainStatus, type LoginState, type ModelPref } from '../../lib/brain';
  import { ACCESS_LEVELS, accessGet, accessSet, auditClear, auditLine, auditRecent, onAccessChanged, onPendingActions, type AccessLevel, type AccessState, type AuditEntry } from '../../lib/actions';
  import { call, callOrThrow, on } from '../../lib/ipc';
  import { data } from '../../lib/store.svelte';
  import Icon from '../Icon.svelte';
  import { update } from '../state.svelte';

  const enabled = $derived(data.settings.modules.brain ?? false);
  const model = $derived((data.settings.brain?.model ?? 'auto') as ModelPref);
  const models: ModelPref[] = ['auto', 'fast', 'long'];

  let status = $state<BrainStatus | null>(null);
  let login = $state<{ state: LoginState; detail: string }>({ state: 'untested', detail: '' });
  let testing = $state(false);

  const refresh = async () => {
    status = enabled ? ((await call<BrainStatus>('brain_status')) ?? null) : null;
  };
  onMount(() => {
    void refresh();
    return on('brain://status', () => void refresh());
  });

  async function toggle(on: boolean) {
    await update(s => (s.modules.brain = on));
    login = { state: 'untested', detail: '' };
    await refresh();
  }

  async function test() {
    testing = true;
    try {
      login = parseLogin(await callOrThrow<string>('brain_check_login'));
    } catch (e) {
      login = { state: 'unknown', detail: String(e) };
    } finally {
      testing = false;
    }
  }


  // Erişim: seviyeyi yalnız bu seçici (fareyle) değiştirir. Modül kapalıyken hiçbir şey yüklenmez.
  let access = $state<AccessState | null>(null);
  let accessError = $state('');
  let recent = $state<AuditEntry[]>([]);
  const level = $derived(access?.level ?? 'closed');

  const loadAccess = async () => {
    if (!enabled) {
      access = null;
      recent = [];
      return;
    }
    access = (await accessGet()) ?? null;
    recent = (await auditRecent(30)) ?? [];
  };
  onMount(() => {
    void loadAccess();
    const offs = [onAccessChanged(() => void loadAccess()), onPendingActions(() => void loadAccess())];
    return () => offs.forEach(off => off());
  });
  $effect(() => {
    void enabled;
    void loadAccess();
  });

  async function pick(next: AccessLevel) {
    accessError = '';
    try {
      access = await accessSet(next);
      recent = (await auditRecent(30)) ?? [];
    } catch (e) {
      accessError = String(e);
    }
  }

  // İzinli klasörler: dosya kopyalama/taşıma eylemleri yalnız buralarda çalışır. Zody bu listeyi değiştiremez.
  const folders = $derived(data.settings.brain?.allowedFolders ?? []);
  async function addFolder() {
    const picked = await call<string | null>('brain_pick_folder');
    if (!picked || folders.some(f => f.toLowerCase() === picked.toLowerCase())) return;
    await update(s => (s.brain = { model: s.brain?.model ?? 'auto', ...s.brain, allowedFolders: [...(s.brain?.allowedFolders ?? []), picked] }));
  }
  const removeFolder = (path: string) => update(s => (s.brain = { model: s.brain?.model ?? 'auto', ...s.brain, allowedFolders: (s.brain?.allowedFolders ?? []).filter(f => f !== path) }));

  async function clearLog() {
    await auditClear();
    recent = [];
  }
  const install = $derived(installLine(enabled ? status : null));
  const loginInfo = $derived(loginLine(enabled ? status : null, login));
</script>

<header class="page-head">
  <h1>Zody asistanı <span class="tag">Deneysel</span></h1>
  <p>Anlaşılmayan istekler, bu bilgisayardaki Claude Code'a (kendi girişinle) gider. Erişim Kapalıyken yalnız konuşur ve "şunu yapmamı ister misin" diye önerir; Düşük/Orta seviyede sınırlı eylemler yapabilir.</p>
</header>

<section class="group">
  <label class="row">
    <span class="ic"><Icon name="ai" /></span>
    <span class="tx">
      <b>Claude aboneliği (bu bilgisayardaki Claude Code)</b>
      <small>Kapalıyken süreç açılmaz ve bellek tutulmaz. Açıkken süreç yalnız soru sorulunca başlar, 5 dakika boşta kalınca kapanır.</small>
    </span>
    <input class="switch" type="checkbox" role="switch" aria-label="Zody asistanı" checked={enabled} onchange={e => toggle(e.currentTarget.checked)} />
  </label>

  <div class="row">
    <span class="ic"><Icon name="info" /></span>
    <span class="tx">
      <b>Claude Code</b>
      <small class="st {install.tone}">{enabled ? install.text : 'Kapalı: yoklanmıyor.'}</small>
    </span>
  </div>

  <div class="row">
    <span class="ic"><Icon name="spark" /></span>
    <span class="tx">
      <b>Giriş</b>
      <small class="st {loginInfo.tone}">{enabled ? loginInfo.text : 'Kapalı: yoklanmıyor.'}</small>
    </span>
    <button class="btn" disabled={!enabled || !status?.installed || testing} onclick={test}>{testing ? 'Deneniyor…' : 'Giriş denemesi'}</button>
  </div>

  <div class="row">
    <span class="ic"><Icon name="motion" /></span>
    <span class="tx">
      <b>Model</b>
      <small>{MODEL_LABELS[model].note}. Sonnet, Haiku'dan yavaş ve daha çok kota harcar.</small>
    </span>
    <span class="seg">
      {#each models as m (m)}
        <button class:on={model === m} disabled={!enabled} onclick={() => update(s => (s.brain = { ...s.brain, model: m }))}>{MODEL_LABELS[m].label}</button>
      {/each}
    </span>
  </div>
</section>

<section class="group">
  <div class="row">
    <span class="ic"><Icon name="info" /></span>
    <span class="tx">
      <b>Erişim</b>
      <small>{ACCESS_LEVELS.find(l => l.id === level)?.note}</small>
      {#if access?.stopped}
        <small class="st warn">Acil durdurma basıldı: bu oturumda Zody Kapalı sayılıyor. Seviyeyi yeniden seçince geri gelir.</small>
      {/if}
      {#if accessError}
        <small class="st bad">{accessError}</small>
      {/if}
    </span>
    <span class="seg">
      {#each ACCESS_LEVELS as l (l.id)}
        <button class:on={level === l.id && !access?.stopped} disabled={!enabled} onclick={() => pick(l.id)}>{l.label}</button>
      {/each}
    </span>
  </div>

  <div class="row">
    <span class="ic"><Icon name="info" /></span>
    <span class="tx">
      <b>İzinli klasörler</b>
      {#if folders.length === 0}
        <small>Yok: dosya kopyalama/taşıma eylemleri bu liste boşken çalışmaz. Dosya silinmez; hedefte aynı ad varsa yeni ad verilir.</small>
      {:else}
        <ul class="log">
          {#each folders as f (f)}
            <li title={f}>{f} <button class="lnk" onclick={() => removeFolder(f)}>Kaldır</button></li>
          {/each}
        </ul>
      {/if}
    </span>
    <button class="btn" disabled={!enabled || folders.length >= 8} onclick={addFolder}>Klasör ekle</button>
  </div>

  <div class="row">
    <span class="ic"><Icon name="info" /></span>
    <span class="tx">
      <b>Son eylemler</b>
      {#if !enabled}
        <small>Kapalı: kayıt yüklenmiyor.</small>
      {:else if recent.length === 0}
        <small>Henüz eylem yok.</small>
      {:else}
        <ul class="log">
          {#each recent as e (e.t + e.eylem + e.onay)}
            <li title={e.ozet}>{auditLine(e)}</li>
          {/each}
        </ul>
      {/if}
      <small>Kayıt: %APPDATA%\Zodyvel\denetim. Şifre ve ekran görüntüsü yazılmaz.</small>
    </span>
    <button class="btn" disabled={!enabled || recent.length === 0} onclick={clearLog}>Kaydı temizle</button>
  </div>
</section>

<p class="note">Zodyvel yalnız resmi Claude Code programını çağırır; Claude kimlik bilgilerine hiç dokunmaz. Asistanın dosya düzenleme ve terminal araçları kapalıdır. Anthropic kuralları değişebilir; deneysel özellik herkese dağıtımdan önce yeniden gözden geçirilecek.</p>

<style>
  .tag {
    font-size: 11px;
    font-weight: 600;
    vertical-align: middle;
    padding: 2px 8px;
    border-radius: 999px;
    color: var(--accent);
    border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .st.ok {
    color: var(--green);
  }
  .st.bad {
    color: var(--red);
  }
  .st.warn {
    color: var(--amber);
  }
  .btn {
    flex: none;
    padding: 6px 12px;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
  .btn:disabled,
  .seg button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .log {
    margin: 4px 0 0;
    padding: 0;
    list-style: none;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    max-height: 168px;
    overflow: auto;
  }
  .log li {
    padding: 1px 0;
    opacity: 0.85;
  }
  .lnk {
    margin-left: 8px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font: inherit;
    cursor: pointer;
  }
  .note {
    margin: 12px 4px 0;
    font-size: 12px;
    opacity: 0.7;
  }
</style>
