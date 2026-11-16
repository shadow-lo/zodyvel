<script lang="ts">
  // Zody: ekran çerçeveli iki göz. Gözün rengi durumu, çerçevenin rengi aracı söyler. Hiçbir animasyon yok (boşta CPU kuralı).
  type Tone = 'accent' | 'amber' | 'green' | 'red' | 'idle';
  let { size = 'md', tone = 'accent', tool = '' }: { size?: 'sm' | 'md' | 'lg' | 'big'; tone?: Tone; tool?: string } = $props();
</script>

<div class="zody {size === 'md' ? '' : size} {tone === 'accent' ? '' : tone} {tool ? 'tool-' + tool : ''}" aria-hidden="true"><i></i><i></i></div>
