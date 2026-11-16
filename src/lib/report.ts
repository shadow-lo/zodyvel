// "Sorun bildir" metni: sürüm, Windows ve ekran bilgisi, açık özellikler ve ayıklanmış debug.log sonu.
// Kişisel veri (yollar, kullanıcı adı, dosya adları) Rust tarafında ayıklanır (redact.rs); burada ayarlardan
// yalnız anahtarlar yazılır, yüklenen ses dosyalarının adları ve kısayollar yazılmaz.

import type { Settings } from './types';

/** Rust `report_info` çıktısı (src-tauri/src/report.rs). */
export interface ReportInfo {
  version: string;
  windows: string;
  screens: string[];
  debugOn: boolean;
  log: string | null;
}

const onOff = (v: boolean) => (v ? 'açık' : 'kapalı');

function section(title: string, lines: string[]): string {
  return `${title}\n${lines.join('\n')}`;
}

export function reportText(info: ReportInfo, settings: Settings, webview: string): string {
  const { modules, island, appearance, features } = settings;
  const head = section('Zodyvel sorun raporu', [
    `Sürüm: ${info.version}`,
    `Windows: ${info.windows}`,
    `Ekran: ${info.screens.length ? info.screens.join(' · ') : 'bilinmiyor'}`,
    `WebView2: ${webview || 'bilinmiyor'}`,
  ]);
  const config = section('Ayarlar', [
    `Görünüm: ${appearance.look === 'new' ? 'Yeni' : 'Klasik'} · tema ${appearance.theme} · zemin ${appearance.surface}`,
    `Modüller: ${Object.entries(modules).map(([k, v]) => `${k} ${onOff(v)}`).join(', ')}`,
    `Ada: açılış ${island.reveal}, üzerine gelince ${island.hover}, selam ${onOff(island.peek)}, hareketi azalt ${onOff(island.reduceMotion)}, ses ${onOff(island.sound)}, bildirim ${onOff(island.flash)}, ekran ${island.screen}, boyut %${island.size}`,
    `Özellikler: ${Object.entries(features).map(([k, v]) => `${k} ${typeof v === 'boolean' ? onOff(v) : v}`).join(', ')}`,
  ]);
  // Ayıklanan kayıt satır sonuyla biter (redact.rs); sondaki boş satır sayılmasın.
  const lines = info.log?.trimEnd() ?? null;
  const log = lines === null
    ? section('debug.log', [info.debugOn ? '(Ayrıntılı kayıt açık; henüz satır yok)' : '(Ayrıntılı kayıt kapalı)'])
    : section(`debug.log (son ${lines ? lines.split('\n').length : 0} satır, kişisel veri ayıklandı)`, [lines || '(boş)']);
  return [head, config, log].join('\n\n') + '\n';
}

/** WebView2 sürümü, kullanıcı aracısından ("Edg/130.0.2849.80"). */
export function webviewVersion(userAgent: string): string {
  return /Edg\/([\d.]+)/.exec(userAgent)?.[1] ?? '';
}
