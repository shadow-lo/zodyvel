import { describe, expect, it } from 'vitest';
import { findDate, findTime, minutesIn, normalize, numberWords, parse } from '../src/lib/commands';

// 22 Ekim 2026 Perşembe, 09:00
const NOW = new Date(2026, 9, 22, 9, 0);
const p = (text: string) => parse(text, NOW);

describe('normalize ve sayılar', () => {
  it('Türkçe küçük harf, noktalama gider', () => {
    expect(normalize('MÜZİĞİ Durdur!')).toBe('müziği durdur');
    expect(normalize('Saat 10.30’da')).toBe("saat 10:30'da");
  });
  it('sözle söylenen sayılar', () => {
    expect(numberWords('yirmi beş dakika')).toBe('25 dakika');
    expect(numberWords('on dakika')).toBe('10 dakika');
    expect(numberWords('yarım saat')).toBe('30 dakika');
    expect(numberWords('bir buçuk saat')).toBe('90 dakika');
    expect(numberWords('bir şarkı')).toBe('bir şarkı');
    expect(minutesIn('1 saat 15 dakika')).toBe(75);
  });
});

describe('zamanlayıcı ve odak', () => {
  it('süreli kurma', () => {
    expect(p('25 dakika zamanlayıcı')).toEqual({ kind: 'timer', minutes: 25 });
    expect(p('yirmi dakikalık sayaç kur')).toEqual({ kind: 'timer', minutes: 20 });
    expect(p('10 dk say')).toEqual({ kind: 'timer', minutes: 10 });
    expect(p('yarım saatlik zamanlayıcı')).toEqual({ kind: 'timer', minutes: 30 });
  });
  it('durdur, başlat, sıfırla, kalan', () => {
    expect(p('zamanlayıcıyı durdur')).toEqual({ kind: 'timerOp', op: 'pause' });
    expect(p('sayacı sıfırla')).toEqual({ kind: 'timerOp', op: 'reset' });
    expect(p('zamanlayıcıyı başlat')).toEqual({ kind: 'timerOp', op: 'start' });
    expect(p('ne kadar kaldı')).toEqual({ kind: 'ask', topic: 'timer' });
  });
  it('odak', () => {
    expect(p('odak modunu başlat')).toEqual({ kind: 'focus', on: true });
    expect(p('pomodoroyu bitir')).toEqual({ kind: 'focus', on: false });
  });
});

describe('müzik ve ses', () => {
  it('müzik', () => {
    expect(p('müziği durdur')).toEqual({ kind: 'music', op: 'pause' });
    expect(p('şarkıyı devam ettir')).toEqual({ kind: 'music', op: 'play' });
    expect(p('sonraki şarkı')).toEqual({ kind: 'music', op: 'next' });
    expect(p('geç')).toEqual({ kind: 'music', op: 'next' });
    expect(p('önceki parça')).toEqual({ kind: 'music', op: 'prev' });
    expect(p('ne çalıyor')).toEqual({ kind: 'ask', topic: 'music' });
  });
  it('tek sözcük durdur/devam belirsiz kalır', () => {
    expect(p('durdur')).toEqual({ kind: 'pause' });
    expect(p('devam et')).toEqual({ kind: 'resume' });
  });
  it('ses düzeyi', () => {
    expect(p('sesi kıs')).toEqual({ kind: 'volume', op: 'down' });
    expect(p('sesi biraz aç')).toEqual({ kind: 'volume', op: 'up' });
    expect(p('sesi yüzde 40 yap')).toEqual({ kind: 'volume', op: 'set', level: 40 });
    expect(p("sesi 30'a ayarla")).toEqual({ kind: 'volume', op: 'set', level: 30 });
    expect(p('sessize al')).toEqual({ kind: 'volume', op: 'mute' });
  });
});

describe('takvim ve yapılacaklar', () => {
  it("yarın 10'da toplantı ekle", () => {
    expect(p("yarın 10'da toplantı ekle")).toEqual({ kind: 'event', title: 'Toplantı', date: '2026-10-23', time: '10:00' });
  });
  it('gün adı, ay adı, akşam', () => {
    expect(p('cuma akşam 7de sinema ekle')).toEqual({ kind: 'event', title: 'Sinema', date: '2026-10-23', time: '19:00' });
    expect(p('takvime 3 kasım dişçi randevusu')).toEqual({ kind: 'event', title: 'Dişçi randevusu', date: '2026-11-03', time: null });
    expect(p('perşembe saat 14:30 sunum koy')).toEqual({ kind: 'event', title: 'Sunum', date: '2026-10-29', time: '14:30' });
  });
  it('saat yorumları', () => {
    expect(findTime("3'te")?.value).toBe('15:00');
    expect(findTime("sabah 9'da")?.value).toBe('09:00');
    expect(findTime('öğlen')?.value).toBe('12:00');
    expect(findTime('25 dakika')).toBeNull();
    expect(findDate('öbür gün', NOW)?.value).toBe('2026-10-24');
    expect(findDate('pazartesi', NOW)?.value).toBe('2026-10-26');
  });
  it('yapılacak', () => {
    expect(p('yapılacaklara süt al ekle')).toEqual({ kind: 'todo', text: 'Süt al' });
    expect(p('faturayı öde diye listeye ekle')).toEqual({ kind: 'todo', text: 'Faturayı öde' });
    expect(p('yapılacaklarda ne var')).toEqual({ kind: 'ask', topic: 'todos' });
  });
  it('sorular', () => {
    expect(p('bugün ne var')).toEqual({ kind: 'ask', topic: 'today' });
    expect(p('yarın ne var')).toEqual({ kind: 'ask', topic: 'tomorrow' });
  });
});

describe('sistem, ajanlar, sohbet', () => {
  it('tema ve radyolar', () => {
    expect(p('koyu temaya geç')).toEqual({ kind: 'dark', on: true });
    expect(p('açık tema')).toEqual({ kind: 'dark', on: false });
    expect(p("wifi'yi kapat")).toEqual({ kind: 'radio', radio: 'wifi', on: false });
    expect(p('bluetooth aç')).toEqual({ kind: 'radio', radio: 'bluetooth', on: true });
    expect(p('bluetooth ayarlarını aç')).toEqual({ kind: 'page', page: 'bluetooth' });
    expect(p('görev yöneticisini aç')).toEqual({ kind: 'open', target: 'taskManager' });
  });
  it('ajanlar ve onaylar', () => {
    expect(p('Claude ne yapıyor?')).toEqual({ kind: 'ask', topic: 'agents' });
    expect(p('ajanlar ne durumda')).toEqual({ kind: 'ask', topic: 'agents' });
    expect(p('onay bekleyen var mı')).toEqual({ kind: 'ask', topic: 'approvals' });
  });
  it('ses ya da yazı asla onay vermez: karta yönlendirilir', () => {
    expect(p('evet')).toEqual({ kind: 'talk', topic: 'approve' });
    expect(p('onayla')).toEqual({ kind: 'talk', topic: 'approve' });
  });
  it('sohbet ve bilinmeyen', () => {
    expect(p('merhaba zody')).toEqual({ kind: 'talk', topic: 'hello' });
    expect(p('saat kaç')).toEqual({ kind: 'ask', topic: 'time' });
    expect(p('teşekkürler')).toEqual({ kind: 'talk', topic: 'thanks' });
    expect(p('pil kaç')).toEqual({ kind: 'ask', topic: 'battery' });
    expect(p('bana bir şiir yaz')).toEqual({ kind: 'unknown' });
  });
});
