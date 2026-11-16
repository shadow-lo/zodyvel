<script lang="ts">
  // Peek: maskot şeridin altından kafasını çıkarır. `wave` ise maskot kendi selamını verir (greet), yanında
  // "Selam!" belirir; değilse yalnız görünür. Her şey bir kez oynar (transform/opacity), sürekli çizim yok.
  // Tıklayınca mini hemen açılır.
  import { Mascot } from '../../mascot';

  let { wave, text = 'Selam!', onopen }: { wave: boolean; text?: string; onopen: () => void } = $props();
</script>

<button class="peek" class:wave onclick={onopen} title="Aç">
  <span class="head">
    <Mascot mood="idle" size={30} alive={wave} greet={wave} />
    {#if wave}
      <svg class="marks" viewBox="0 0 12 12" aria-hidden="true">
        <path d="M3 3.2 Q5.6 2 7.4 3.8 M2.4 6.4 Q6 5 9.4 7.6" />
      </svg>
    {/if}
  </span>
  {#if wave}<b class="hi">{text}</b>{/if}
</button>

<style>
  .peek {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    border-radius: 0 0 20px 20px;
  }
  .head {
    position: relative;
    display: grid;
    flex: none;
  }
  .marks {
    position: absolute;
    top: -3px;
    right: -9px;
    width: 12px;
    height: 12px;
    opacity: 0;
  }
  .marks path {
    fill: none;
    stroke: var(--accent-2);
    stroke-width: 1.4;
    stroke-linecap: round;
  }
  .hi {
    font-size: var(--fs-12-5);
    font-weight: 650;
    color: var(--text);
    white-space: nowrap;
  }
  @media (prefers-reduced-motion: no-preference) {
    /* Şeridin içinden aşağı süzülür, hafif yaylanır. */
    .head {
      animation: rise var(--t-slow) var(--spring) both;
    }
    .marks {
      animation: marks 520ms steps(7) 160ms both;
    }
    .hi {
      animation: slide var(--t-base) var(--ease) 160ms both;
    }
  }
  @keyframes rise {
    from {
      transform: translateY(-130%);
    }
  }
  @keyframes marks {
    0%,
    100% {
      opacity: 0;
    }
    30%,
    70% {
      opacity: 1;
    }
  }
  @keyframes slide {
    from {
      transform: translateX(-6px);
      opacity: 0;
    }
  }
</style>
