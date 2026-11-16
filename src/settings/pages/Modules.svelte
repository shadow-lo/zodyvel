<script lang="ts">
  // Modüller: kapalı modül adada görünmez ve sistemden veri okumaz.
  import { data } from '../../lib/store.svelte';
  import Icon, { type IconName } from '../Icon.svelte';
  import { go, ui, update } from '../state.svelte';

  const modules: { id: 'agents' | 'music' | 'today' | 'agenda' | 'modes' | 'system'; icon: IconName; title: string; text: string }[] = [
    { id: 'agents', icon: 'ai', title: 'AI ajanları', text: 'Claude Code ve Codex oturumları, adımları, onaylar ve sorular.' },
    { id: 'music', icon: 'music', title: 'Müzik ve Kontroller', text: 'Çalan şarkı ve sarma; Wi-Fi, Bluetooth, ses düzeyi, koyu mod.' },
    { id: 'today', icon: 'today', title: 'Bugün', text: 'Saat, yapılacaklar ve en çok üç Zodymetre (veri bu bilgisayarda kalır).' },
    { id: 'agenda', icon: 'today', title: 'Ajanda', text: 'Saat saat planlar: tekrar (her gün, hafta içi, seçili günler), Gün/Ay görünümü, başlarken çentikte haber. Veri bu bilgisayarda kalır.' },
    { id: 'modes', icon: 'clock', title: 'Modlar', text: 'Çalışma, Ders, Uyku, Oyun ve kendi modların: bildirim süzgeci, müzik, uygulama kapat/aç, Zody sessiz, odak döngüsü.' },
    { id: 'system', icon: 'pc', title: 'Bilgisayar', text: 'CPU, GPU, RAM ve en çok bellek kullananlar; yalnız ekrandayken ölçülür.' },
  ];

  // AI kapalıyken Zodyvel köprü olaylarını yok sayar; araçlardaki kayıtlar kalırsa köprü her olayda bir anlığına
  // açılıp kapanır. Kaldırmak kullanıcının seçimi (Bağlantılar'da önizleme + yedekle), kendiliğinden yapılmaz.
  const names = { claude: 'Claude Code', codex: 'Codex' } as const;
  const hooked = $derived(ui.connections.filter(c => c.installed).map(c => names[c.provider]));
</script>

<header class="page-head">
  <h1>Modüller</h1>
  <p>Kapalı bir modül adada görünmez ve sistemden hiçbir veri okumaz.</p>
</header>

<section class="group">
  {#each modules as m (m.id)}
    <label class="row">
      <span class="ic"><Icon name={m.icon} /></span>
      <span class="tx">
        <b>{m.title}</b><small>{m.text}</small>
        {#if m.id === 'agents' && !data.settings.modules.agents && hooked.length}
          <span class="warn">
            {hooked.join(' ve ')} bağlantısı hâlâ kurulu: her olayda köprü bir anlığına açılıp kapanır. Tamamen durdurmak için
            <button class="link" onclick={() => go('baglanti')}>Bağlantılar'dan kaldır</button>.
          </span>
        {/if}
      </span>
      <input class="switch" type="checkbox" role="switch" checked={data.settings.modules[m.id]} onchange={e => update(s => (s.modules[m.id] = e.currentTarget.checked))} />
    </label>
  {/each}
</section>
