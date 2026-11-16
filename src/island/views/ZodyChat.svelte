<script lang="ts">
  // Zody sohbeti (plan 3.2): üst çubuktaki Zody sekmesi. Solda canlı Zody, sağda mesajlar ve yazı kutusu.
  // Minide son birkaç mesaj, ayrıntıda geçmişin tamamı. Komutları bugün kural beyni anlar (chat.ts); ileride
  // model (ve bas-konuş) aynı yere bağlanır. Onay yalnız karttaki düğmeyle verilir.
  // Hareket: yalnız transform/opacity, tek seferlik (yeni balon, Zody'nin konuşması); düşünürken üç nokta.
  // Boştayken Zody'nin kendi (adımlı) canlılığı dışında hiçbir şey oynamaz.
  import { tick } from 'svelte';
  import Zody from '../../mascot/Zody.svelte';
  import { EXAMPLES } from '../../lib/commands';
  import { mascotMood, moodLabel, type Card, type ChatMessage } from '../../lib/chat';
  import { brainLabel, chat, clearChat, decide, say } from '../../lib/chat.svelte';
  import { startTyping } from '../../lib/store.svelte';

  interface Props {
    detail: boolean;
  }
  let { detail }: Props = $props();

  let box = $state<HTMLInputElement>();
  let list = $state<HTMLDivElement>();

  const shown = $derived<ChatMessage[]>(detail ? chat.messages : chat.messages.slice(-3));
  const empty = $derived(chat.messages.length === 0);
  const mood = $derived(mascotMood(chat.mood));
  const caption = $derived(moodLabel(chat.mood));

  // Yeni mesaj gelince en alta kaydır (ayrıntıda geçmiş uzun olabilir).
  $effect(() => {
    void chat.messages.length;
    void tick().then(() => list?.scrollTo({ top: list.scrollHeight, behavior: 'smooth' }));
  });

  async function send(text = chat.draft) {
    const t = text.trim();
    if (!t || chat.busy) return;
    chat.draft = '';
    await say(t);
    await tick();
    box?.focus();
  }

  function chip(text: string) {
    startTyping();
    void send(text);
  }

  let lastMine = $derived([...chat.messages].reverse().find(m => m.from === 'me')?.text ?? '');
  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void send();
    } else if (e.key === 'ArrowUp' && !chat.draft && lastMine) {
      e.preventDefault();
      chat.draft = lastMine;
    }
  }

  const cardText: Record<Card['state'], string> = {
    waiting: '',
    allowed: 'Onaylandı',
    denied: 'Vazgeçildi',
    expired: 'Süresi doldu',
    failed: 'Olmadı',
  };
</script>

<div class="chat" class:detail>
  <section class="stage" aria-label="Zody">
    <div class="halo">
      {#key chat.spoke}
        <span class="bob" class:talk={chat.spoke > 0}>
          <Zody {mood} size={detail ? 96 : 62} alive interactive />
        </span>
      {/key}
    </div>
    {#if detail}<b class="name">Zody</b>{/if}
    <span class="cap" aria-live="polite">{caption}</span>
    {#if detail}
      <span class="brain" title="Komutları şimdilik kurallar anlıyor; model bağlanınca burada adı görünür.">{brainLabel()} · çevrimdışı</span>
    {/if}
  </section>

  <section class="talk">
    {#if detail}
      <header class="head">
        <span class="ttl">Sohbet</span>
        <span class="sp"></span>
        {#if !empty}<button class="ghost" onclick={clearChat}>Temizle</button>{/if}
      </header>
    {/if}
    <div class="msgs" class:fade={!detail && chat.messages.length > 2} bind:this={list} role="log" aria-label="Zody ile sohbet">
      {#if empty}
        <div class="row z enter">
          <p class="bubble z">Selam! Ben Zody. Ne yapalım?</p>
        </div>
        <div class="chips enter">
          {#each EXAMPLES.slice(0, detail ? 6 : 3) as ex (ex)}
            <button class="chip" onclick={() => chip(ex)}>{ex}</button>
          {/each}
        </div>
      {/if}
      {#each shown as m (m.id)}
        <div class="row enter" class:me={m.from === 'me'} class:z={m.from === 'zody'}>
          <div class="bubble" class:me={m.from === 'me'} class:z={m.from === 'zody'} data-tone={m.tone}>
            <span>{m.text}</span>
            {#if m.card}
              <!-- Minide kart tek satır: Zody'nin cümlesi + düğmeler; ayrıntılı satır ayrıntıda. -->
              <div class="card" class:slim={!detail} data-state={m.card.state}>
                {#if detail}<span class="sum">{m.card.summary}</span>{/if}
                {#if m.card.state === 'waiting'}
                  <div class="acts">
                    <button class="no" onclick={() => m.card && decide(m.card, false)}>Vazgeç</button>
                    <button class="yes" onclick={() => m.card && decide(m.card, true)}>Onayla</button>
                  </div>
                {:else}
                  <span class="st">{cardText[m.card.state]}{m.card.note ? ` · ${m.card.note}` : ''}</span>
                {/if}
              </div>
            {/if}
          </div>
        </div>
        {#if m.chips?.length && m === chat.messages[chat.messages.length - 1]}
          <div class="chips enter">
            {#each m.chips as c (c)}<button class="chip" onclick={() => chip(c)}>{c}</button>{/each}
          </div>
        {/if}
      {/each}
      {#if chat.busy}
        <div class="row z enter"><span class="bubble z dots" aria-label="Düşünüyor"><i></i><i></i><i></i></span></div>
      {/if}
    </div>

    <form class="input" onsubmit={e => (e.preventDefault(), void send())}>
      <input
        bind:this={box}
        bind:value={chat.draft}
        maxlength="500"
        placeholder="Zody'ye yaz…"
        aria-label="Zody'ye yaz"
        autocomplete="off"
        spellcheck="false"
        {onkeydown}
        onfocus={() => (startTyping(), (chat.typing = true))}
        onblur={() => (chat.typing = false)}
      />
      <button type="submit" class="send" class:ready={chat.draft.trim() !== ''} disabled={!chat.draft.trim() || chat.busy} aria-label="Gönder">
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12h13M12.5 6.5 18 12l-5.5 5.5" /></svg>
      </button>
    </form>
  </section>
</div>

<style>
  .chat {
    height: calc(100% - 36px);
    display: flex;
    gap: 12px;
    margin: 0 10px 10px;
    padding: 10px 10px 10px 6px;
    border-radius: 18px;
    background: var(--surface);
    border: 1px solid var(--line);
    box-shadow: var(--card-depth);
    background-image: var(--card-sheen);
  }
  .detail {
    padding: 14px 14px 14px 8px;
    gap: 16px;
  }

  /* Sol: Zody sahnesi */
  .stage {
    flex: none;
    width: 104px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }
  .detail .stage {
    width: 150px;
    gap: 4px;
    padding-bottom: 24px;
  }
  .halo {
    position: relative;
    display: grid;
    place-items: center;
    width: 84px;
    height: 84px;
    border-radius: 50%;
    background: radial-gradient(circle, color-mix(in srgb, var(--accent) 24%, transparent) 0, transparent 70%);
  }
  .detail .halo {
    width: 128px;
    height: 128px;
    margin-bottom: 6px;
  }
  .bob {
    display: block;
    line-height: 0;
  }
  /* Zody cevap verince bir kez "konuşur": hafif zıplayıp yerine oturur. */
  .bob.talk {
    animation: talk var(--t-slow) var(--spring) both;
  }
  @keyframes talk {
    0% {
      transform: translateY(0) scale(1);
    }
    35% {
      transform: translateY(-4px) scale(1.04, 0.97);
    }
    70% {
      transform: translateY(1px) scale(0.99, 1.02);
    }
    100% {
      transform: none;
    }
  }
  .name {
    font-size: var(--fs-14);
    font-weight: 600;
    letter-spacing: 0.01em;
  }
  .cap {
    font-size: var(--fs-11);
    color: var(--text-3);
    text-align: center;
    line-height: 1.25;
    max-width: 100%;
  }
  .brain {
    margin-top: 8px;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: var(--fs-10-5);
    color: var(--text-3);
    border: 1px solid var(--line);
  }

  /* Sağ: sohbet */
  .talk {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .head {
    display: flex;
    align-items: center;
    height: 22px;
  }
  .ttl {
    font-size: var(--fs-12);
    color: var(--text-2);
    font-weight: 600;
  }
  .sp {
    flex: 1;
  }
  .ghost {
    font-size: var(--fs-11);
    color: var(--text-3);
    padding: 2px 8px;
    border-radius: 8px;
  }
  .ghost:hover {
    color: var(--text-2);
    background: var(--surface-2);
  }
  .msgs {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 2px 4px 2px 0;
    scrollbar-width: thin;
    scrollbar-color: var(--surface-3) transparent;
  }
  /* Minide eski mesajlar yukarıda silikleşir (statik maske). */
  .fade {
    mask-image: linear-gradient(to bottom, transparent 0, #000 26px);
  }
  .msgs > :first-child {
    margin-top: auto;
  }
  .row {
    display: flex;
  }
  .row.me {
    justify-content: flex-end;
  }
  .enter {
    animation: pop var(--t-base) var(--ease) both;
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.97);
    }
  }
  .row.me.enter {
    transform-origin: right bottom;
  }
  .row.z.enter {
    transform-origin: left bottom;
  }
  .bubble {
    margin: 0;
    max-width: 86%;
    padding: 6px 11px 7px;
    border-radius: 15px;
    font-size: var(--fs-12-5);
    line-height: 1.38;
    overflow-wrap: anywhere;
  }
  .bubble.z {
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-bottom-left-radius: 5px;
    color: var(--text);
  }
  .bubble.me {
    background: color-mix(in srgb, var(--accent) 26%, var(--surface-2));
    border: 1px solid color-mix(in srgb, var(--accent) 34%, transparent);
    border-bottom-right-radius: 5px;
    color: var(--text);
  }
  .bubble[data-tone='warn'] {
    border-color: color-mix(in srgb, var(--amber) 40%, transparent);
  }
  .bubble[data-tone='error'] {
    border-color: color-mix(in srgb, var(--red) 45%, transparent);
  }
  .detail .bubble {
    max-width: 78%;
  }

  /* Onay kartı: tek onay yolu. */
  .card {
    margin-top: 7px;
    padding: 8px 9px;
    border-radius: 11px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  .card.slim {
    margin-top: 5px;
    padding: 0;
    background: none;
    border: 0;
  }
  .card.slim .acts {
    justify-content: flex-start;
  }
  .sum {
    font-size: var(--fs-11-5);
    color: var(--text-2);
  }
  .acts {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
  }
  .acts button {
    height: 26px;
    padding: 0 12px;
    border-radius: 8px;
    font-size: var(--fs-12);
  }
  .no {
    color: var(--text-2);
    background: var(--surface-2);
  }
  .no:hover {
    background: var(--surface-3);
  }
  .yes {
    color: var(--on-accent);
    background: var(--accent-soft);
    font-weight: 600;
  }
  .yes:hover {
    background: color-mix(in srgb, var(--accent-soft), white 12%);
  }
  .st {
    font-size: var(--fs-11);
    color: var(--text-3);
  }
  .card[data-state='allowed'] .st {
    color: var(--green);
  }
  .card[data-state='failed'] .st,
  .card[data-state='expired'] .st {
    color: var(--amber);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    padding-left: 2px;
  }
  .chip {
    height: 24px;
    padding: 0 10px;
    border-radius: 999px;
    font-size: var(--fs-11-5);
    color: var(--accent-soft);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 26%, transparent);
  }
  .chip:hover {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }

  /* Düşünürken üç nokta (yalnız cevap beklerken). */
  .dots {
    display: inline-flex;
    gap: 4px;
    padding: 9px 12px;
  }
  .dots i {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--text-3);
    animation: dot 900ms var(--ease) infinite;
  }
  .dots i:nth-child(2) {
    animation-delay: 150ms;
  }
  .dots i:nth-child(3) {
    animation-delay: 300ms;
  }
  @keyframes dot {
    0%,
    100% {
      opacity: 0.35;
      transform: translateY(0);
    }
    40% {
      opacity: 1;
      transform: translateY(-3px);
    }
  }

  /* Yazı kutusu */
  .input {
    flex: none;
    display: flex;
    align-items: center;
    gap: 4px;
    height: 38px;
    padding: 0 4px 0 13px;
    border-radius: 13px;
    background: var(--island);
    border: 1px solid var(--line-strong);
  }
  .input:focus-within {
    border-color: color-mix(in srgb, var(--accent-soft) 60%, transparent);
  }
  .input input {
    flex: 1;
    min-width: 0;
    height: 100%;
    font: inherit;
    font-size: var(--fs-12-5);
    color: var(--text);
    background: none;
    border: 0;
    outline: none;
  }
  .input input::placeholder {
    color: var(--text-3);
  }
  .send {
    flex: none;
    width: 30px;
    height: 30px;
    border-radius: 10px;
    display: grid;
    place-items: center;
    color: var(--text-3);
  }
  .send svg {
    width: 17px;
    height: 17px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.9;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .send.ready {
    color: var(--on-accent);
    background: var(--accent-soft);
  }
  .send:disabled {
    cursor: default;
  }
</style>
