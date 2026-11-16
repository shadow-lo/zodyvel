import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

/** Tarayıcı önizlemesinde (pnpm dev) Tauri yoktur; çağrılar sessizce atlanır. */
export const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export async function call<T = void>(cmd: string, args?: Record<string, unknown>): Promise<T | undefined> {
  if (!inTauri) return undefined;
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    console.warn(`${cmd} başarısız`, error);
    return undefined;
  }
}

/** Hata mesajını çağırana ileten sürüm (ör. "Bu istek artık geçerli değil."). */
export async function callOrThrow<T = void>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri) throw new Error('Tauri dışında çalışıyor');
  return invoke<T>(cmd, args);
}

export function on<T>(event: string, handler: (payload: T) => void): () => void {
  if (!inTauri) return () => {};
  const off = listen<T>(event, e => handler(e.payload));
  return () => void off.then(stop => stop());
}
