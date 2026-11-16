import { describe, expect, it } from 'vitest';
import { keyName, readCombo, type KeyLike } from '../src/lib/shortcuts';

const key = (keyCode: number, mods: Partial<Omit<KeyLike, 'keyCode'>> = {}): KeyLike => ({
  keyCode,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...mods,
});

describe('kısayol kaydedici', () => {
  it('birleşimi Rust ile aynı sırada yazar', () => {
    expect(readCombo(key(90, { altKey: true, ctrlKey: true }))).toEqual({ kind: 'combo', combo: 'Ctrl+Alt+Z' });
    expect(readCombo(key(123, { shiftKey: true, metaKey: true }))).toEqual({ kind: 'combo', combo: 'Shift+Win+F12' });
    expect(readCombo(key(13, { ctrlKey: true, altKey: true }))).toEqual({ kind: 'combo', combo: 'Ctrl+Alt+Enter' });
  });

  it('yalnız değiştirici tuşa basılınca bekler, Esc iptal eder', () => {
    expect(readCombo(key(17, { ctrlKey: true })).kind).toBe('wait');
    expect(readCombo(key(27)).kind).toBe('cancel');
  });

  it('tek tuşu, yalnız Shift birleşimini ve bilinmeyen tuşu kabul etmez', () => {
    expect(readCombo(key(90)).kind).toBe('invalid');
    expect(readCombo(key(90, { shiftKey: true })).kind).toBe('invalid');
    expect(readCombo(key(219, { ctrlKey: true, altKey: true })).kind).toBe('invalid'); // Türkçe Ğ (OEM)
  });

  it('tuş adları', () => {
    expect(keyName(48)).toBe('0');
    expect(keyName(112)).toBe('F1');
    expect(keyName(135)).toBe('F24');
    expect(keyName(9)).toBeNull();
  });
});
