import { describe, expect, it } from 'vitest';
import { answerReply, failureReply, installLine, loginLine, parseLogin, pickModel, type BrainStatus } from '../src/lib/brain';

const status = (over: Partial<BrainStatus> = {}): BrainStatus => ({ enabled: true, installed: true, version: '2.1.0 (Claude Code)', running: false, name: 'Claude aboneliği', ...over });

describe('model seçimi', () => {
  it('elle seçim her zaman uyulur', () => {
    expect(pickModel('bir şeyler yaz', 'fast')).toBe('fast');
    expect(pickModel('selam', 'long')).toBe('long');
  });
  it('otomatik: kısa soru hızlı, uzun iş uzun', () => {
    expect(pickModel('Hava nasıl olur?', 'auto')).toBe('fast');
    expect(pickModel('Bu e-postayı yazar mısın', undefined)).toBe('long');
    expect(pickModel('Şu raporu özetle', 'auto')).toBe('long');
    expect(pickModel('a'.repeat(200), 'auto')).toBe('long');
  });
});

describe('durum metinleri', () => {
  it('kurulu değilse açıkça söyler', () => {
    expect(installLine(status({ installed: false, version: null })).tone).toBe('bad');
    expect(loginLine(status({ installed: false }), parseLogin(undefined)).tone).toBe('idle');
  });
  it('giriş durumları', () => {
    expect(loginLine(status(), parseLogin('yes')).tone).toBe('ok');
    expect(loginLine(status(), parseLogin('no')).text).toContain('/login');
    expect(parseLogin('unknown:ağ hatası')).toEqual({ state: 'unknown', detail: 'ağ hatası' });
    expect(loginLine(status(), parseLogin('unknown:ağ hatası')).tone).toBe('warn');
  });
  it('sürüm gösterilir', () => {
    expect(installLine(status()).text).toContain('2.1.0');
    expect(installLine(null).tone).toBe('idle');
  });
});

describe('cevaplar eylem yapmaz', () => {
  it('beyin cevabında adım yok', () => {
    expect(answerReply('Şunu yapmamı ister misin?').steps).toBeUndefined();
    expect(failureReply('kapalı').steps).toBeUndefined();
  });
  it('uzun cevap kısaltılır, boş cevap düşer', () => {
    expect(answerReply('x'.repeat(5000)).text.length).toBeLessThan(2100);
    expect(answerReply('  ').text).toBe('Bir şey diyemedim.');
  });
});
