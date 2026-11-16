// Ayarlar penceresinde basılan tuşlardan kısayol metni üretir. Biçim Rust'taki
// shortcuts.rs ile aynıdır ("Ctrl+Alt+Z"); Rust kaydederken yeniden doğrular.
//
// Tuş, Windows sanal tuş koduyla (keyCode) okunur: RegisterHotKey de aynı kodu kullanır, bu yüzden
// Türkçe Q/F gibi düzenlerde de basılan harf ile kaydedilen harf aynıdır.

const NAMED: Record<number, string> = {
  8: 'Backspace',
  13: 'Enter',
  32: 'Space',
  33: 'PageUp',
  34: 'PageDown',
  35: 'End',
  36: 'Home',
  37: 'Left',
  38: 'Up',
  39: 'Right',
  40: 'Down',
  45: 'Insert',
  46: 'Delete',
};

/** Shift, Ctrl, Alt, Win (sol/sağ), CapsLock: tek başına basılınca birleşimin tamamlanması beklenir. */
const MODIFIER_KEYS = new Set([16, 17, 18, 20, 91, 92]);

export function keyName(vk: number): string | null {
  if ((vk >= 48 && vk <= 57) || (vk >= 65 && vk <= 90)) return String.fromCharCode(vk);
  if (vk >= 112 && vk <= 135) return `F${vk - 111}`;
  return NAMED[vk] ?? null;
}

export interface KeyLike {
  keyCode: number;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

export type ReadResult =
  | { kind: 'combo'; combo: string }
  | { kind: 'cancel' }
  | { kind: 'wait' }
  | { kind: 'invalid'; reason: string };

export function readCombo(e: KeyLike): ReadResult {
  const mods = e.ctrlKey || e.altKey || e.metaKey;
  if (e.keyCode === 27 && !mods && !e.shiftKey) return { kind: 'cancel' };
  if (MODIFIER_KEYS.has(e.keyCode)) return { kind: 'wait' };
  const key = keyName(e.keyCode);
  if (!key) return { kind: 'invalid', reason: 'Bu tuş kısayolda kullanılamaz. Harf, rakam, F1–F24 veya Enter, Boşluk gibi bir tuş seç.' };
  if (!mods) return { kind: 'invalid', reason: 'Ctrl, Alt veya Win ile birlikte bas: tek tuş yazı yazarken çakışır.' };
  const parts = [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Win', key].filter(Boolean);
  return { kind: 'combo', combo: parts.join('+') };
}
