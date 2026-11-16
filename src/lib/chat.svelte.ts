// Zody sohbetinin durumu ve yürütücüsü. Mesajlar yalnız bellekte durur (uygulama kapanınca silinir).
// Beyin cevabındaki adımlar eylem kaydından geçer: read/safe hemen çalışır, confirm sohbete kart düşer.
// Kartın Onayla/Vazgeç düğmesi tek onay yoludur; yazı (ileride ses) onay veremez (actions.ts).
// Bas-konuş ertelendi (analiz/bas-konus/rapor.md): eklenince sözü `say(text, 'voice')` ile buraya verir.

import { decideAction, onPendingActions, requestAction, type ActionOutcome } from './actions';
import { MAX_MESSAGES, needsVolume, rulesBrain, think, type Brain, type Card, type ChatMessage, type ChatMood, type Reply, type World } from './chat';
import { claudeBrain } from './brain.svelte';
import { call, inTauri } from './ipc';
import { context, data } from './store.svelte';
import type { ControlsState } from './types';

/** Zincir: önce kurallar. İleride model beyni buraya eklenir (anlaşılmayan cümleler ona gider). */
const brains: Brain[] = [rulesBrain, claudeBrain];

let nextId = 1;

export const chat = $state({
  messages: [] as ChatMessage[],
  mood: 'idle' as ChatMood,
  busy: false,
  /** Kutuda yazı var ya da odakta: mini ada fare çıkınca kapanmaz. */
  typing: false,
  draft: '',
  /** Son Zody mesajı geldiğinde artar: maskot bir kez "konuşur". */
  spoke: 0,
});

export const brainLabel = () => brains.map(b => b.label).join(' + ');

/** Mini ada fare çıkınca kapanmasın: yazıyor ya da cevap bekliyor. */
export const chatHolds = () => chat.typing || chat.draft.trim() !== '' || chat.busy;

function push(message: Omit<ChatMessage, 'id' | 'at'>): ChatMessage {
  const full = { ...message, id: nextId++, at: Date.now() };
  chat.messages = [...chat.messages, full].slice(-MAX_MESSAGES);
  return full;
}

function patchCard(id: number, patch: Partial<Card>) {
  chat.messages = chat.messages.map(m => (m.card?.id === id ? { ...m, card: { ...m.card, ...patch } } : m));
}

let moodTimer: ReturnType<typeof setTimeout> | undefined;
function setMood(mood: ChatMood, ms = 0) {
  clearTimeout(moodTimer);
  chat.mood = mood;
  if (ms) moodTimer = setTimeout(() => (chat.mood = 'idle'), ms);
}

async function world(text: string): Promise<World> {
  const ctx = context();
  const m = data.settings.modules;
  let volume: World['volume'];
  if (needsVolume(text)) volume = inTauri ? ((await call<ControlsState>('controls_read'))?.volume ?? null) : { level: 42, muted: false, device: null };
  return {
    now: new Date(),
    agents: ctx.agents,
    approvals: ctx.approvals,
    music: ctx.music,
    today: ctx.today,
    battery: ctx.battery,
    volume,
    modules: { agents: m.agents, music: m.music, today: m.today, system: m.system, modes: m.modes },
  };
}

// Tarayıcı önizlemesi (pnpm dev): eylem kaydı yok; onaylı eylemler sahte kart, diğerleri "yapıldı".
const DEMO_CONFIRM = new Set(['today.event_add', 'controls.radio', 'today.todo_remove', 'shelf.remove', 'agents.note', 'agents.task', 'startup.set', 'app.open', 'app.close', 'files.copy', 'files.move', 'window.type', 'mail.draft']);
let demoId = 100;
async function request(action: string, args: Record<string, unknown> | undefined, source: string): Promise<ActionOutcome> {
  if (inTauri) return requestAction(action, args, source);
  if (!DEMO_CONFIRM.has(action)) return { status: 'done', result: null };
  const summary = action === 'today.event_add' ? `Takvime etkinlik ekle · Başlık: ${args?.title}, Tarih: ${args?.date}${args?.time ? `, Saat: ${args.time}` : ''}` : action;
  return { status: 'pending', request: { id: ++demoId, action, risk: 'confirm', summary, source, createdMs: Date.now() } };
}

/** Onaylanınca söylenecekler (kart numarasına göre). */
const afterAllow = new Map<number, { text: string; rest: Reply['steps']; source: string }>();

async function run(reply: Reply, source: string) {
  const steps = reply.steps ?? [];
  for (let i = 0; i < steps.length; i++) {
    const step = steps[i]!;
    let outcome: ActionOutcome;
    try {
      outcome = await request(step.action, step.args, source);
    } catch (error) {
      setMood('confused', 1600);
      push({ from: 'zody', text: typeof error === 'string' ? error : error instanceof Error ? error.message : 'Olmadı, bir şeyler ters gitti.', tone: 'error' });
      return;
    }
    if (outcome.status === 'denied') {
      setMood('confused', 1600);
      push({ from: 'zody', text: 'Bu işi yapmama izin yok.', tone: 'warn' });
      return;
    }
    if (outcome.status === 'pending') {
      const card: Card = { id: outcome.request.id, summary: outcome.request.summary, state: 'waiting' };
      afterAllow.set(card.id, { text: reply.text, rest: steps.slice(i + 1), source });
      setMood('idle');
      push({ from: 'zody', text: reply.confirmText ?? 'Bunu yapmadan önce onayın lazım:', tone: 'info', card });
      return;
    }
  }
  setMood(reply.mood ?? 'idle', reply.mood && reply.mood !== 'idle' ? 1400 : 0);
  push({ from: 'zody', text: reply.text, tone: reply.tone, chips: reply.chips });
  chat.spoke++;
}

/** Kullanıcının sözü (yazı ya da bas-konuş). */
export async function say(text: string, via: 'text' | 'voice' = 'text') {
  const clean = text.trim().slice(0, 500);
  if (!clean || chat.busy) return;
  push({ from: 'me', text: clean, ...(via === 'voice' && { via }) });
  chat.busy = true;
  setMood('thinking');
  try {
    const reply = await think(brains, { text: clean, via, world: await world(clean) });
    await run(reply, via === 'voice' ? 'voice' : 'zody');
  } finally {
    chat.busy = false;
    if (chat.mood === 'thinking') setMood('idle');
  }
}

/** Kartın düğmesi: yalnız fare ya da klavye (hiçbir zaman ses). */
export async function decide(card: Card, allow: boolean) {
  if (card.state !== 'waiting') return;
  const after = afterAllow.get(card.id);
  afterAllow.delete(card.id);
  if (!allow) {
    if (inTauri) await decideAction(card.id, false).catch(() => {});
    patchCard(card.id, { state: 'denied' });
    push({ from: 'zody', text: 'Tamam, vazgeçtim.' });
    return;
  }
  try {
    if (inTauri) await decideAction(card.id, true);
    patchCard(card.id, { state: 'allowed' });
    if (after) await run({ text: after.text, steps: after.rest, mood: 'happy' }, after.source);
  } catch (error) {
    const note = typeof error === 'string' ? error : 'Bu istek artık geçerli değil.';
    patchCard(card.id, { state: 'failed', note });
    setMood('confused', 1600);
  }
}

export function clearChat() {
  chat.messages = [];
  afterAllow.clear();
  setMood('idle');
}

/** Bekleyen kartın süresi dolarsa (2 dk) kart "süresi doldu" olur. */
export function watchChat(): () => void {
  return onPendingActions(list => {
    const live = new Set(list.map(p => p.id));
    for (const m of chat.messages) if (m.card?.state === 'waiting' && !live.has(m.card.id)) patchCard(m.card.id, { state: 'expired' });
  });
}
