import { describe, expect, it } from 'vitest';
import { reportText, webviewVersion, type ReportInfo } from '../src/lib/report';
import { defaultSettings } from '../src/lib/types';

const info: ReportInfo = { version: '0.6.0', windows: 'Windows 11 Home 24H2 (derleme 26100.2894)', screens: ['1920×1080 · %125 (birincil)'], debugOn: true, log: '1 ilk\n2 son' };

describe('sorun raporu', () => {
  it('sürüm, Windows, ekran, özellikler ve kayıt sonu içerir', () => {
    const text = reportText(info, defaultSettings, '130.0.2849.80');
    expect(text).toContain('Sürüm: 0.6.0');
    expect(text).toContain('Windows: Windows 11 Home 24H2');
    expect(text).toContain('Ekran: 1920×1080 · %125 (birincil)');
    expect(text).toContain('WebView2: 130.0.2849.80');
    expect(text).toContain('shelf açık');
    expect(text).toContain('weeklyRecap kapalı');
    expect(text).toContain('waitAlertMin 10');
    expect(text).toContain('son 2 satır');
    expect(text.endsWith('2 son\n')).toBe(true);
  });

  it('kayıt sonundaki satır sonu sayılmaz; ada ekranı ve boyutu yazılır', () => {
    const text = reportText({ ...info, log: '1 ilk\n2 son\n' }, defaultSettings, '');
    expect(text).toContain('son 2 satır');
    expect(text.endsWith('2 son\n')).toBe(true);
    expect(text).toContain('ekran primary, boyut %100');
  });

  it('yüklenen ses adları ve kısayollar rapora girmez', () => {
    const settings = structuredClone(defaultSettings);
    settings.sounds.custom = { done: 'ali-ozel-sarki.wav' };
    const text = reportText(info, settings, '');
    expect(text).not.toContain('ali-ozel-sarki');
    expect(text).not.toContain('Ctrl+Alt');
  });

  it('kayıt yoksa nedenini söyler', () => {
    expect(reportText({ ...info, debugOn: false, log: null }, defaultSettings, '')).toContain('(Ayrıntılı kayıt kapalı)');
    expect(reportText({ ...info, debugOn: true, log: null }, defaultSettings, '')).toContain('henüz satır yok');
  });

  it('WebView2 sürümünü okur', () => {
    expect(webviewVersion('Mozilla/5.0 Chrome/130.0.0.0 Safari/537.36 Edg/130.0.2849.80')).toBe('130.0.2849.80');
    expect(webviewVersion('Firefox')).toBe('');
  });
});
