// Sohbet zincirinin ikinci halkası: kurallar anlamazsa Zody asistanına (Claude aboneliği) sorar.
// Modül kapalıysa hiçbir şey çağrılmaz (null: sıradaki beyin / "anlamadım").

import { answerReply, failureReply, pickModel, type ModelPref } from './brain';
import type { Brain } from './chat';
import { callOrThrow, inTauri } from './ipc';
import { data } from './store.svelte';

export const claudeBrain: Brain = {
  id: 'claude',
  label: 'Claude',
  async reply(turn) {
    if (!inTauri || !data.settings.modules.brain) return null;
    const model = pickModel(turn.text, data.settings.brain?.model as ModelPref | undefined);
    try {
      return answerReply(await callOrThrow<string>('brain_ask', { text: turn.text, model }));
    } catch (error) {
      return failureReply(typeof error === 'string' ? error : error instanceof Error ? error.message : 'bilinmeyen hata');
    }
  },
};
