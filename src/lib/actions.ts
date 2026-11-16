// Eylem kaydı ve genel onay merkezi (Rust: src-tauri/src/actions). Zody'nin isteği buradan geçer:
// `requestAction` onaysız eylemi hemen çalıştırır, onaylıyı bekleyen istek yapar; kart `decideAction`
// ile karar verir. Arayüzün kendi düğmeleri komutları eskisi gibi doğrudan çağırır.

import { call, callOrThrow, on } from './ipc';

/** read ve safe onaysız çalışır; confirm her seferinde kart ister; internal Zody'ye kapalıdır. */
export type ActionRisk = 'read' | 'safe' | 'confirm' | 'internal';

export type ActionArgKind =
  | { type: 'bool' }
  | { type: 'int'; min: number; max: number }
  | { type: 'text'; max: number }
  | { type: 'choice'; options: string[] }
  | { type: 'date' }
  | { type: 'time' };

export interface ActionArg {
  name: string;
  label: string;
  kind: ActionArgKind;
  optional: boolean;
}

export interface ActionSpec {
  /** Kalıcı ad, ör. "music.control". */
  name: string;
  command: string;
  title: string;
  risk: ActionRisk;
  args: ActionArg[];
}

export interface PendingAction {
  id: number;
  action: string;
  risk: ActionRisk;
  /** Onay kartı satırı. */
  summary: string;
  source: string;
  createdMs: number;
}

export type ActionOutcome =
  | { status: 'done'; result: unknown }
  | { status: 'pending'; request: PendingAction }
  | { status: 'denied' };

export const listActions = () => call<ActionSpec[]>('actions_list');

export const pendingActions = () => call<PendingAction[]>('actions_pending');

/** Hata mesajı (ör. "Dakika: geçersiz değer.") çağırana gider; Zody onu söyleyebilir. */
export const requestAction = (action: string, args?: Record<string, unknown>, source = 'zody') =>
  callOrThrow<ActionOutcome>('actions_request', { action, args: args ?? null, source });

/** Yalnız onay kartı çağırır; Zody ve ses asla onay vermez. */
export const decideAction = (id: number, allow: boolean) => callOrThrow<ActionOutcome>('actions_decide', { id, allow });

export const onPendingActions = (handler: (list: PendingAction[]) => void) => on<PendingAction[]>('actions://pending', handler);

// --- Erişim seviyesi ve denetim kaydı (Faz 13). Yalnız arayüz çağırır; Zody bu komutları göremez. ---

export type AccessLevel = 'closed' | 'low' | 'mid';

export interface AccessState {
  /** Ayardaki seviye. */
  level: AccessLevel;
  /** Şu an geçerli seviye (acil durdurma sonrası "closed"). */
  effective: AccessLevel;
  /** Acil durdurma bu oturumda basıldı. */
  stopped: boolean;
}

export const ACCESS_LEVELS: { id: AccessLevel; label: string; note: string }[] = [
  { id: 'closed', label: 'Kapalı', note: 'Zody yalnız konuşur ve önerir; hiçbir eylem yapmaz.' },
  { id: 'low', label: 'Düşük', note: 'Okuma ve geri alınabilir işler: tarayıcıda bağlantı/arama açma, yeni Claude sohbeti, müzik/ses, Zodymetre, kısayol uygulamaları. Onay istemez; her eylem kayda geçer.' },
  { id: 'mid', label: 'Orta', note: 'Düşük + daha kapsamlı işler (uygulama açma/kapatma, izinli klasörde dosya kopyalama/taşıma, etkin pencereye yazma, mail taslağı, takvime ekleme, Wi-Fi/Bluetooth, ajana görev). Kritik her eylem onay kartı ister; 2 dakikada cevap yoksa iptal.' },
];

export interface AuditEntry {
  t: number;
  zaman: string;
  istek: string;
  seviye: AccessLevel;
  eylem: string;
  ozet: string;
  onay: string;
  onayMs: number;
  sonuc: string;
}

export const accessGet = () => call<AccessState>('access_get');
export const accessSet = (level: AccessLevel) => callOrThrow<AccessState>('access_set', { level });
export const onAccessChanged = (handler: (s: AccessState) => void) => on<AccessState>('access://changed', handler);
export const auditRecent = (limit = 30) => call<AuditEntry[]>('audit_recent', { limit });
export const auditClear = () => call<number>('audit_clear');

/** Denetim satırı: "14:03 · music.control · ok". */
export function auditLine(e: AuditEntry): string {
  const onay = e.onay === 'gerekmedi' ? '' : ` · onay: ${e.onay}`;
  return `${e.zaman.slice(0, 5)} · ${e.eylem}${onay} · ${e.sonuc}`;
}
