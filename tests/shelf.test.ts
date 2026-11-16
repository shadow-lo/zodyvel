import { describe, expect, it } from 'vitest';
import { dropText, folderLabel, kindLabel, outside, pastThreshold, sendChip, sentText, sessionSub, shelfVisible } from '../src/lib/shelf';
import { defaultFeatures, defaultSettings } from '../src/lib/types';

describe('dosya rafı', () => {
  it('varsayılan açık; eski ayar dosyasında da', () => {
    expect(defaultFeatures().shelf).toBe(true);
    expect(defaultSettings.features.shelf).toBe(true);
  });

  it('kapalıyken bölüm hiç görünmez (rafta dosya olsa da, sürükleme olsa da)', () => {
    expect(shelfVisible(false, 3, false)).toBe(false);
    expect(shelfVisible(false, 0, true)).toBe(false);
    expect(shelfVisible(true, 0, false)).toBe(false);
    expect(shelfVisible(true, 0, true)).toBe(true);
    expect(shelfVisible(true, 2, false)).toBe(true);
  });

  it('etiketler', () => {
    expect(kindLabel({ name: 'rapor.final.pdf', dir: false })).toBe('PDF');
    expect(kindLabel({ name: 'Proje', dir: true })).toBe('Klasör');
    expect(kindLabel({ name: 'Makefile', dir: false })).toBe('Dosya');
    expect(kindLabel({ name: '.env', dir: false })).toBe('Dosya');
    expect(folderLabel('C:\\Users\\a\\Belgeler\\x.pdf')).toBe('…\\a\\Belgeler');
    expect(folderLabel('D:\\x.pdf')).toBe('D:');
    expect(dropText(2)).toBe('2 dosya rafa eklendi');
    expect(dropText(0)).toBe('Zaten rafta');
  });

  it('tıklama sürükleme sayılmaz', () => {
    expect(pastThreshold(2, 2)).toBe(false);
    expect(pastThreshold(3, 3)).toBe(true);
  });
});

describe('raf → AI (K5)', () => {
  it('satır alt yazısı: Zodyvel görevi, masaüstü, VS Code, araç', () => {
    expect(sessionSub({ tool: 'claude', taskNo: 2, app: null, attach: true })).toBe('Zodyvel görevi #2');
    expect(sessionSub({ tool: 'claude', taskNo: 2, app: null, attach: false })).toBe('Claude Code');
    expect(sessionSub({ tool: 'claude', app: 'Claude.exe' })).toBe('Claude masaüstü');
    expect(sessionSub({ tool: 'codex', app: 'Code.exe' })).toBe('Codex · VS Code');
    expect(sessionSub({ tool: 'claude', app: 'WindowsTerminal.exe' })).toBe('Claude Code');
  });

  it('çip ne olacağını söyler, imleç üstündeyken bırak der', () => {
    expect(sendChip(true, false)).toBe('dosya eklenir');
    expect(sendChip(false, false)).toBe('yolu kopyala');
    expect(sendChip(true, true)).toBe('Bırak → eklenir');
    expect(sendChip(false, true)).toBe('Bırak → kopyala');
  });

  it('gönderim notu', () => {
    const base = { how: 'attached' as const, focused: false, count: 1, name: 'rapor.pdf' };
    expect(sentText(base, 'Psrc')).toBe('rapor.pdf → Psrc · göreve eklendi');
    expect(sentText({ ...base, how: 'copied', focused: true, count: 3 }, 'api')).toBe('3 dosya → api · yol kopyalandı, pencere öne geldi');
    expect(sentText({ ...base, how: 'copied' }, 'api')).toBe('rapor.pdf → api · yol kopyalandı');
    expect(sentText({ ...base, error: 'Bu oturum artık yok.' }, 'api')).toBe('api: Bu oturum artık yok.');
  });

  it('kart adadan çıkınca Windows sürüklemesine geçer', () => {
    const r = { left: 10, top: 10, right: 110, bottom: 60 };
    expect(outside(50, 30, r)).toBe(false);
    expect(outside(114, 30, r)).toBe(false);
    expect(outside(120, 30, r)).toBe(true);
    expect(outside(50, 0, r)).toBe(true);
  });
});
