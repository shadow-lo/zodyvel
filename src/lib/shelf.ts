// Dosya rafı (Özellikler → "Dosya rafı"): saf yardımcılar. Sürükle-bırak işini Rust yapar (src-tauri/src/shelf.rs).

import type { AgentSession, ShelfItem, ShelfSent } from './types';

/** Raftan dışarı sürükleme bu kadar piksel kımıldayınca başlar (tıklama sürükleme sayılmaz). */
export const DRAG_PX = 4;

/** Raf bölümü simge çubuğunda görünür mü: özellik açık ve rafta dosya var ya da şu an dosya sürükleniyor. */
export function shelfVisible(on: boolean, items: number, dragging: boolean): boolean {
  return on && (items > 0 || dragging);
}

/** Dosya kartındaki kısa tür etiketi ("rapor.final.pdf" → "PDF", klasör → "Klasör"). */
export function kindLabel(item: Pick<ShelfItem, 'name' | 'dir'>): string {
  if (item.dir) return 'Klasör';
  const dot = item.name.lastIndexOf('.');
  if (dot <= 0 || dot === item.name.length - 1) return 'Dosya';
  return item.name.slice(dot + 1, dot + 6).toUpperCase();
}

/** Yolun bulunduğu klasör, kısaltılmış ("C:\Users\a\Belgeler\x.pdf" → "…\a\Belgeler"). */
export function folderLabel(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  parts.pop();
  if (parts.length <= 2) return parts.join('\\') || path;
  return `…\\${parts.slice(-2).join('\\')}`;
}

/** Bırakma notu: "3 dosya rafa eklendi" / "Zaten rafta". */
export function dropText(count: number): string {
  return count > 0 ? `${count} dosya rafa eklendi` : 'Zaten rafta';
}

/** Pointer hareketi sürükleme eşiğini geçti mi. */
export function pastThreshold(dx: number, dy: number): boolean {
  return Math.hypot(dx, dy) >= DRAG_PX;
}

// ---------- Raf → AI (Faz 6, K5) ----------

/** "Oturuma gönder" listesindeki satırın alt yazısı: Zodyvel görevi mi, hangi araç/pencere. */
export function sessionSub(a: Pick<AgentSession, 'tool' | 'taskNo' | 'app' | 'attach'>): string {
  if (a.attach) return a.taskNo ? `Zodyvel görevi #${a.taskNo}` : 'Zodyvel görevi';
  const exe = (a.app ?? '').toLowerCase();
  if (exe === 'claude.exe') return 'Claude masaüstü';
  if (exe === 'code.exe' || exe === 'code - insiders.exe') return a.tool === 'codex' ? 'Codex · VS Code' : 'Claude Code · VS Code';
  return a.tool === 'codex' ? 'Codex' : 'Claude Code';
}

/** Satırın sağındaki kısa çip: ne olacağı; imleç üstündeyken "Bırak → …". */
export function sendChip(attach: boolean, hot: boolean): string {
  if (hot) return attach ? 'Bırak → eklenir' : 'Bırak → kopyala';
  return attach ? 'dosya eklenir' : 'yolu kopyala';
}

/** Gönderim notu: "rapor.pdf → Psrc · göreve eklendi". */
export function sentText(sent: Pick<ShelfSent, 'how' | 'focused' | 'count' | 'name' | 'error'>, target: string): string {
  if (sent.error) return `${target}: ${sent.error}`;
  const what = sent.count > 1 ? `${sent.count} dosya` : sent.name;
  const how = sent.how === 'attached' ? 'göreve eklendi' : sent.focused ? 'yol kopyalandı, pencere öne geldi' : 'yol kopyalandı';
  return `${what} → ${target} · ${how}`;
}

/** Nokta dikdörtgenin (payla genişletilmiş) dışında mı: raftaki kart adadan çıkınca Windows sürüklemesine geçer. */
export function outside(x: number, y: number, r: { left: number; top: number; right: number; bottom: number }, pad = 6): boolean {
  return x < r.left - pad || x > r.right + pad || y < r.top - pad || y > r.bottom + pad;
}
