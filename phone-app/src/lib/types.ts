// Telefon ↔ çekirdek sözleşmesi: md/telefon-t4c.md (§3 ve §4).
export interface AppState {
  phase: 'none' | 'paired';
  device_secure: boolean;
  desktop_name: string | null;
  fingerprint: string | null;
  unlocked: boolean;
}
export interface PairBegin {
  code: string;
  fingerprint: string;
}
export interface Probe {
  sessions: number;
  approvals: number;
}
export interface StepInfo {
  /** read | edit | bash | search | other */
  kind: string;
  label: string;
  target: string;
}
export interface SessionInfo {
  id: string;
  /** claude | codex | other */
  tool: string;
  name: string;
  /** Klasör adı (yol değil); boş olabilir. */
  folder: string;
  /** running | waiting_input | waiting_approval | done | error (bilinmeyen = idle) */
  status: string;
  /** Son tur / oturum başlangıcı, unix saniye. */
  started: number | null;
  /** Aşağıdakiler yalnız ayrıntı penceresi (2 dk) açıkken dolu. */
  step: string | null;
  steps: StepInfo[] | null;
  reply: string | null;
}
export interface QuestionInfo {
  q: string;
  options: string[];
  multi: boolean;
}
export interface ApprovalInfo {
  id: string;
  /** Oturum adı. */
  session: string;
  /** Oturum kimliği. */
  session_id: string;
  /** permission | question | plan */
  kind: string;
  /** Yalnız ayrıntı penceresi açıkken dolu. */
  action: string | null;
  questions: QuestionInfo[] | null;
}
export interface LiveState {
  enabled: boolean;
  running: boolean;
  /** off | connecting | searching | connected | retry | repair | revoked */
  link: string;
  detail: string;
  approvals: ApprovalInfo[];
  sessions: SessionInfo[];
  /** Ayrıntı penceresinin bitişi (ms epoch); kapalıysa null. */
  unlocked_until: number | null;
  /** Son başarılı bağlantı (ms epoch). */
  last_connected: number | null;
  /** Masaüstünün O9 kararı: donanım korumalı mı (bilinmiyorsa null). */
  hw: boolean | null;
  /** strongbox | tee | "" */
  hw_level: string | null;
  /** G3: Ajanda hatırlatıcıları açık mı. */
  reminders: boolean;
  /** G3: kurulu hatırlatma sayısı (son eşitleme); hiç eşitlenmediyse null. */
  reminder_count: number | null;
  /** G3: telefon açılınca bağlan. */
  autostart: boolean;
}
export interface FolderItem {
  id: string;
  name: string;
}
export interface AgendaItem {
  id: number;
  /** "2026-11-14" */
  date: string;
  /** "14:00"; yoksa gün boyu. */
  time: string | null;
  title: string;
  minutes: number;
  lead: number;
  notify: boolean;
}
export interface AgendaView {
  from: string;
  days: number;
  items: AgendaItem[];
}
