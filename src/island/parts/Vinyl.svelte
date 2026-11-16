<script lang="ts">
  // Müzik çalarken dönen plak. Yalnız transform döner; duraklatınca animasyon tamamen durur.
  //
  // Ölçüm (144 Hz ekran): sürekli (linear) dönüş WebView2 GPU sürecini tek çekirdeğin ~%18'ine
  // çıkarıyordu. steps() ile saniyede ~8 adım, değişmeyen kareleri Chromium'un atlamasını sağlar
  // ve maliyeti kabaca beşte bire indirir. JS ile döndürmek daha pahalı (her adımda yeniden boyama).
  interface Props {
    playing: boolean;
    size?: number;
  }
  let { playing, size = 18 }: Props = $props();
</script>

<span class="vinyl" class:spin={playing} style:--s="{size}px" role="img" aria-label={playing ? 'Müzik çalıyor' : 'Müzik duraklatıldı'}>
  <span class="label"></span>
</span>

<style>
  .vinyl {
    position: relative;
    display: inline-block;
    width: var(--s);
    height: var(--s);
    border-radius: 50%;
    flex: none;
    background:
      conic-gradient(from 20deg, transparent 0 16%, rgb(255 255 255 / 0.2) 22%, transparent 28% 66%, rgb(255 255 255 / 0.13) 72%, transparent 78%),
      repeating-radial-gradient(circle, #0d0d10 0 1px, #24252b 1px 2px);
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.08);
  }
  .label {
    position: absolute;
    inset: 32%;
    border-radius: 50%;
    background: var(--coral);
    box-shadow: inset 0 0 0 1.5px rgb(0 0 0 / 0.35);
  }
  .label::after {
    content: '';
    position: absolute;
    inset: 38%;
    border-radius: 50%;
    background: #0b0c10;
  }
  @media (prefers-reduced-motion: no-preference) {
    .spin {
      animation: spin 3.6s steps(29) infinite;
    }
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
