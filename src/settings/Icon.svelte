<script lang="ts" module>
  // Ayarlar penceresinin çizgi simgeleri (kendi çizimimiz; yazı tipi ya da paket yok).
  export type IconName =
    | 'home' | 'palette' | 'island' | 'grid' | 'bell' | 'link' | 'key' | 'power' | 'info'
    | 'ai' | 'music' | 'today' | 'pc' | 'folder' | 'motion' | 'sound' | 'spark' | 'battery' | 'clock'
    | 'plan' | 'chart' | 'search' | 'phone';
</script>

<script lang="ts">
  let { name, size = 17 }: { name: IconName; size?: number } = $props();
</script>

<svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
  {#if name === 'home'}
    <path d="M3 11l9-7 9 7v9a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z" />
  {:else if name === 'palette'}
    <circle cx="12" cy="12" r="9" /><circle class="f" cx="8" cy="10" r="1.3" /><circle class="f" cx="12" cy="7.5" r="1.3" /><circle class="f" cx="16" cy="10" r="1.3" /><path d="M12 21a3 3 0 0 1 0-6h2" />
  {:else if name === 'island'}
    <rect x="5" y="3" width="14" height="6" rx="3" /><path d="M3 21h18M7 14h10" />
  {:else if name === 'grid'}
    <rect x="3" y="3" width="7" height="7" rx="2" /><rect x="14" y="3" width="7" height="7" rx="2" /><rect x="3" y="14" width="7" height="7" rx="2" /><rect x="14" y="14" width="7" height="7" rx="2" />
  {:else if name === 'bell'}
    <path d="M6 8a6 6 0 0 1 12 0c0 7 3 8 3 8H3s3-1 3-8" /><path d="M10 20a2 2 0 0 0 4 0" />
  {:else if name === 'link'}
    <path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1" /><path d="M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1" />
  {:else if name === 'key'}
    <rect x="2" y="7" width="20" height="11" rx="2" /><path d="M6 11h.01M10 11h.01M14 11h.01M18 11h.01M7 15h10" />
  {:else if name === 'power'}
    <path d="M12 3v8" /><path d="M6.4 6.6a8 8 0 1 0 11.2 0" />
  {:else if name === 'info'}
    <circle cx="12" cy="12" r="9" /><path d="M12 11v5M12 8h.01" />
  {:else if name === 'ai'}
    <circle cx="12" cy="12" r="5.2" /><path d="M2.8 13.4c2.6 2.2 15.8 -1.6 18.4 -5.2" />
  {:else if name === 'music'}
    <path d="M9 18V5l11-2v13" /><circle cx="6" cy="18" r="3" /><circle cx="17" cy="16" r="3" />
  {:else if name === 'today'}
    <rect x="3.5" y="5" width="17" height="15" rx="2.5" /><path d="M3.5 10h17M8 3v4M16 3v4" />
  {:else if name === 'pc'}
    <rect x="3" y="4" width="18" height="12" rx="2" /><path d="M8 20h8M12 16v4" />
  {:else if name === 'folder'}
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
  {:else if name === 'motion'}
    <path d="M4 12h3l2-5 4 10 2-5h5" />
  {:else if name === 'sound'}
    <path d="M4 9h4l5-4v14l-5-4H4z" /><path d="M16.5 8.5a5 5 0 0 1 0 7M19 6a8.5 8.5 0 0 1 0 12" />
  {:else if name === 'battery'}
    <rect x="3" y="7" width="16" height="10" rx="2.5" /><path d="M22 11v2M7 10.5v3M10.5 10.5v3" />
  {:else if name === 'clock'}
    <circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 2" />
  {:else if name === 'plan'}
    <rect x="5" y="4" width="14" height="17" rx="2.5" /><path d="M9 4.5V3h6v1.5M8.5 10l1.5 1.5L13 8.5M8.5 16h7" />
  {:else if name === 'chart'}
    <path d="M4 20h16M7 16v-4M12 16V6M17 16v-7" />
  {:else if name === 'phone'}
    <rect x="7" y="2.5" width="10" height="19" rx="2.5" /><path d="M11 18.5h2" />
  {:else if name === 'search'}
    <circle cx="11" cy="11" r="6.5" /><path d="M16 16l4.5 4.5" />
  {:else}
    <path d="M12 3l2.2 5.8L20 11l-5.8 2.2L12 19l-2.2-5.8L4 11l5.8-2.2z" />
  {/if}
</svg>

<style>
  svg {
    flex: none;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .f {
    fill: currentColor;
    stroke: none;
  }
</style>
