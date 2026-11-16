import { describe, expect, it } from 'vitest';
import { codeDigits, errText } from '../src/lib/format';

describe('format', () => {
  it('altı haneli kodu kutulara böler', () => {
    expect(codeDigits('482916')).toEqual(['4', '8', '2', '9', '1', '6']);
  });
  it('geçersiz kodda boş kutu gösterir', () => {
    expect(codeDigits('12')).toEqual(Array(6).fill('·'));
    expect(codeDigits('12345a')).toEqual(Array(6).fill('·'));
  });
  it('hata metnini kısaltır ve boşsa genel mesaj verir', () => {
    expect(errText('x'.repeat(500))).toHaveLength(200);
    expect(errText(undefined)).toContain('Tekrar dene');
    expect(errText(new Error('Ağ yok'))).toBe('Ağ yok');
  });
});
