<script lang="ts">
  // Çizgi simgeler (design/telefon-eskiz.html ile aynı yollar).
  const P: Record<string, string> = {
    list: '<path d="M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01"/>',
    plus: '<path d="M12 6v12M6 12h12"/>',
    cal: '<rect x="4" y="5.5" width="16" height="14.5" rx="2.5"/><path d="M4 10h16M8.5 3.5v4M15.5 3.5v4"/>',
    gear: '<circle cx="12" cy="12" r="3"/><path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1"/>',
    read: '<path d="M6 4h9l3 3v13H6z"/>',
    edit: '<path d="M5 19l1-4L16 5l3 3L9 18z"/>',
    bash: '<path d="M5 8l4 4-4 4M12 17h7"/>',
    search: '<circle cx="11" cy="11" r="6"/><path d="M20 20l-4.5-4.5"/>',
    other: '<circle cx="12" cy="12" r="1.5"/><circle cx="6" cy="12" r="1.5"/><circle cx="18" cy="12" r="1.5"/>',
    folder: '<path d="M3.5 7.5A1.5 1.5 0 0 1 5 6h4l2 2h8a1.5 1.5 0 0 1 1.5 1.5v8A1.5 1.5 0 0 1 19 19H5a1.5 1.5 0 0 1-1.5-1.5z"/>',
    send: '<path d="M4 12l16-8-6 16-2.5-6.5z"/>',
    finger: '<path d="M8 11a4 4 0 0 1 8 0v2a10 10 0 0 1-1.5 5M12 11v3a12 12 0 0 1-2 6M6 14v-3a6 6 0 0 1 10-4.5M18 12.5v.5a14 14 0 0 1-.6 4"/>',
    check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
    stop: '<rect x="6.5" y="6.5" width="11" height="11" rx="2"/>',
    play: '<path d="M7 5v14l11-7z"/>',
    shield: '<path d="M12 3l7 3v6c0 4.5-3 7.5-7 9-4-1.5-7-4.5-7-9V6z"/><path d="M9 12l2 2 4-4"/>',
    lock: '<rect x="5" y="11" width="14" height="9" rx="2"/><path d="M8 11V8a4 4 0 0 1 8 0v3"/>',
    eye: '<path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12z"/><circle cx="12" cy="12" r="2.5"/>',
    bell: '<path d="M6 16V11a6 6 0 0 1 12 0v5l1.5 2h-15z"/><path d="M10 20.5a2 2 0 0 0 4 0"/>',
    phone: '<rect x="7" y="2.5" width="10" height="19" rx="2.5"/><path d="M11 18.5h2"/>',
    wifi: '<path d="M2.5 9a14 14 0 0 1 19 0M5.5 12.5a9.5 9.5 0 0 1 13 0M8.7 16a5 5 0 0 1 6.6 0"/><circle cx="12" cy="19" r="1"/>',
    timer: '<circle cx="12" cy="13" r="7.5"/><path d="M12 9v4M10 2.5h4"/>',
    chev: '<path d="M9 6l6 6-6 6"/>',
    back: '<path d="M15 6l-6 6 6 6"/>',
    restart: '<path d="M4 12a8 8 0 1 0 2.3-5.6M4 4v4h4"/>',
  };
  let { name, size = 18 }: { name: string; size?: number } = $props();
</script>

<svg class="ico" viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">{@html P[name] ?? P.other}</svg>
