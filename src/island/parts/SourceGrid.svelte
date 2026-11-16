<script lang="ts">
  // Kaynaklar ızgarası (mini adanın sağı, Özellikler → Kaynaklar): en çok 6 kutucuk. Önce açık
  // kaynaklar (Mail, GitHub; yerleri sabit), sonra her Claude/Codex oturumu ayrı kutucuk. Kutucuk = Zody
  // (çerçeve kaynağın renginde, gözler durum) + ad. Çipin kenarı kaynağın renginde %30 (Faz 1 §2.3) + sayı rozeti ya da kısa durum. Sol kartta duran kutucuk çerçeveli. Dokunma: birinci
  // sol kartı ona çevirir, ikinci ayrıntıyı açar (çağıran).
  import { hub, type HubView } from '../../lib/sources.svelte';
  import { TILE_COLORS, type TileId } from '../../lib/sources';
  import SourceZody from './SourceZody.svelte';

  interface Props {
    view: HubView;
    onpick: (id: TileId) => void;
    /** "+ Kaynak ekle" (Ayarlar › Kaynaklar); boş yer varsa görünür. */
    onadd?: () => void;
  }
  let { view, onpick, onadd }: Props = $props();
</script>

<section class="card grid" aria-label="Kaynaklar">
  {#each view.tiles as t (t.id)}
    {@const name = t.hint ?? t.label}
    <button class="tile {t.tone}" class:on={view.shown === t.id} style:--k={TILE_COLORS[t.kind]} onclick={() => onpick(t.id)} title={t.count ? `${name} · ${t.count}` : t.note ? `${name} · ${t.note}` : name}>
      <SourceZody tile={t.kind} tone={t.tone} size={26} bump={hub.bump?.id === t.id ? hub.bump.key : null} mood={t.tone === 'alert' ? 'error' : t.tone === 'busy' ? 'working' : 'idle'} />
      {#if t.hint}
        <!-- Oturum: proje adı üstte, durum altta (ikisi yan yana sığmıyor). -->
        <span class="two"><span class="nm">{t.label}</span><small class="note">{t.note}</small></span>
      {:else}
        <span class="nm">{t.label}</span>
        {#if t.count}<b class="badge">{t.count > 99 ? '99+' : t.count}</b>{:else if t.note}<small class="note">{t.note}</small>{/if}
      {/if}
    </button>
  {/each}
  {#if onadd && view.tiles.length < 6}
    <button class="tile add" onclick={onadd}>+ Kaynak ekle</button>
  {/if}
  {#if view.noSessions}<small class="paused">Çalışan Claude oturumu yok</small>{/if}
  {#if hub.paused}<small class="paused">Kaynaklar duraklatıldı (tepsi)</small>{/if}
</section>

<style>
  .card {
    background: var(--glass);
    border: 1px solid var(--line);
    box-shadow: var(--card-depth);
    border-radius: var(--card-r);
  }
  .grid {
    flex: none;
    width: 304px;
    padding: 8px;
    display: grid;
    grid-template-columns: 1fr 1fr;
    grid-auto-rows: 34px;
    align-content: start;
    gap: 6px;
  }
  .tile {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 0 8px 0 4px;
    border-radius: 11px;
    background: linear-gradient(180deg, color-mix(in srgb, var(--k) 9%, transparent), rgb(255 255 255 / 0.02));
    border: 1px solid color-mix(in srgb, var(--k) 30%, transparent);
    font-size: var(--fs-12);
    color: var(--text);
    text-align: left;
    transition: background var(--t-fast), border-color var(--t-fast);
  }
  .tile:hover {
    background: linear-gradient(180deg, color-mix(in srgb, var(--k) 16%, transparent), rgb(255 255 255 / 0.03));
  }
  .tile.on {
    border-color: color-mix(in srgb, var(--k) 65%, transparent);
    box-shadow: 0 0 12px color-mix(in srgb, var(--k) 22%, transparent);
  }
  .nm {
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .two {
    flex: 1;
    min-width: 0;
    display: grid;
    line-height: 1.15;
  }
  .two .note {
    text-align: left;
    font-size: var(--fs-10-5);
  }
  .badge {
    flex: none;
    margin-left: auto;
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    border-radius: 999px;
    display: grid;
    place-items: center;
    font-size: var(--fs-10-5);
    font-variant-numeric: tabular-nums;
    font-weight: 700;
    color: var(--on-amber);
    background: var(--amber);
    box-shadow: 0 0 8px color-mix(in srgb, var(--amber) 60%, transparent);
  }
  .note {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: right;
    font-size: var(--fs-11);
    color: var(--text-2);
  }
  .alert .note {
    color: var(--red);
  }
  .busy .note {
    color: var(--teal);
  }
  .off .nm,
  .off .note {
    color: var(--text-3);
  }
  .add {
    justify-content: center;
    --k: var(--text-3);
    color: var(--text-2);
    background: transparent;
    border: 1px dashed var(--line-strong);
  }
  .paused {
    grid-column: 1 / -1;
    font-size: var(--fs-11);
    color: var(--text-3);
  }
</style>
