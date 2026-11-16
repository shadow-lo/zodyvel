<script lang="ts">
  // Tanıtım turu (ilk açılışta, ya da Ayarlar → Ada → "Turu yeniden göster"): mini boyutunda dört-beş kısa adım.
  // Maskot ilk adımda selam verir (greet), her adımda kısa bir tepki verir.
  import { Mascot } from '../../mascot';
  import { tourSteps } from '../../lib/peek';
  import { data } from '../../lib/store.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const steps = $derived(tourSteps(data.settings.shortcuts, data.settings.features));
  let at = $state(0);
  const current = $derived(steps[at] ?? steps[0]!);
  const last = $derived(at === steps.length - 1);
  const next = () => (last ? onclose() : at++);
</script>

<div class="welcome">
  <div class="stage">
    {#key at}
      <span class="hero" class:first={at === 0}><Mascot mood={current.mood} size={64} alive greet={at === 0} /></span>
    {/key}
  </div>
  <div class="body">
    {#key at}
      <div class="copy">
        <small class="count">{at + 1} / {steps.length}</small>
        <b>{current.title}</b>
        <p>{current.text}</p>
      </div>
    {/key}
    <div class="foot">
      <span class="dots" aria-hidden="true">
        {#each steps as _, i (i)}<i class:on={i === at}></i>{/each}
      </span>
      {#if !last}<button class="ghost" onclick={onclose}>Atla</button>{/if}
      {#if at > 0}<button class="ghost" onclick={() => at--}>Geri</button>{/if}
      <button class="primary" onclick={next}>{last ? 'Başlayalım' : 'İleri'}</button>
    </div>
  </div>
</div>

<style>
  .welcome {
    width: 100%;
    height: 100%;
    display: flex;
    gap: 6px;
    padding: 16px 20px 14px 10px;
  }
  .stage {
    width: 130px;
    flex: none;
    display: grid;
    place-items: center;
  }
  .hero {
    display: grid;
    transform-origin: 50% 85%;
  }
  .body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .copy {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .count {
    font-size: var(--fs-11);
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .copy b {
    font-size: var(--fs-16);
    font-weight: 650;
  }
  .copy p {
    margin: 2px 0 0;
    font-size: var(--fs-12-5);
    line-height: 1.45;
    color: var(--text-2);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .dots {
    flex: 1;
    display: flex;
    gap: 5px;
  }
  .dots i {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--surface-3);
    transition: background var(--t-base) var(--ease), transform var(--t-base) var(--ease);
  }
  .dots i.on {
    background: var(--accent);
    transform: scale(1.25);
  }
  .foot button {
    height: 28px;
    padding: 0 12px;
    border-radius: 9px;
    font-size: var(--fs-12);
    font-weight: 600;
  }
  .ghost {
    color: var(--text-2);
  }
  .ghost:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  .primary {
    background: var(--accent);
    color: var(--on-accent);
  }
  .primary:hover {
    background: var(--accent-soft);
  }
  @media (prefers-reduced-motion: no-preference) {
    /* İlk adım: şeridin içinden süzülür, el sallar. Sonraki adımlar: küçük bir zıplama. */
    .hero {
      animation: hop var(--t-slow) var(--spring) both;
    }
    .hero.first {
      animation: rise 560ms var(--spring) both;
    }
    .copy {
      animation: slide var(--t-base) var(--ease) both;
    }
  }
  @keyframes rise {
    from {
      transform: translateY(-150%);
    }
  }
  @keyframes hop {
    40% {
      transform: translateY(-10%) scale(1.04);
    }
  }
  @keyframes slide {
    from {
      transform: translateX(8px);
      opacity: 0;
    }
  }
</style>
