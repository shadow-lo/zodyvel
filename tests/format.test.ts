import { describe, expect, it } from 'vitest';
import { duration, lineCounts, windowLabel } from '../src/lib/format';

describe('onay kartı yazıları', () => {
  it('pencere düğmesi uygulamaya göre adlanır', () => {
    expect(windowLabel('WindowsTerminal.exe')).toBe('Terminale git');
    expect(windowLabel('Code.exe')).toBe("VS Code'a git");
    expect(windowLabel('claude.exe')).toBe("Claude'a git");
    expect(windowLabel(null)).toBe('Terminale git');
  });

  it('satır sayıları tipografik eksiyle', () => {
    expect(lineCounts(12, 3)).toBe('+12 −3');
  });
});

describe('tur süresi', () => {
  it('kısa sürede saniye, uzunda dakika ve saat', () => {
    expect(duration(42_000)).toBe('42 sn');
    expect(duration(252_000)).toBe('4 dk 12 sn');
    expect(duration(300_000)).toBe('5 dk');
    expect(duration(18 * 60_000 + 20_000)).toBe('18 dk');
    expect(duration(65 * 60_000)).toBe('1 sa 5 dk');
    expect(duration(120 * 60_000)).toBe('2 sa');
  });
});
