export const percent = (value: number) => `%${Math.round(value)}`;

export const gb = (value: number) => `${value.toLocaleString('tr-TR', { maximumFractionDigits: 1 })} GB`;

export const mb = (value: number) =>
  value >= 1024 ? gb(value / 1024) : `${Math.round(value).toLocaleString('tr-TR')} MB`;

export function clock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${String(s).padStart(2, '0')}`;
}

/** Tur süresi: "42 sn", "4 dk 12 sn", "18 dk", "1 sa 5 dk". */
export function duration(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000));
  if (total < 60) return `${total} sn`;
  const minutes = Math.floor(total / 60);
  if (minutes < 10) return total % 60 ? `${minutes} dk ${total % 60} sn` : `${minutes} dk`;
  if (minutes < 60) return `${minutes} dk`;
  const rest = minutes % 60;
  return rest ? `${Math.floor(minutes / 60)} sa ${rest} dk` : `${Math.floor(minutes / 60)} sa`;
}

export function since(timestamp: number, now = Date.now()): string {
  const minutes = Math.max(0, Math.round((now - timestamp) / 60000));
  if (minutes < 1) return 'şimdi';
  if (minutes < 60) return `${minutes} dk`;
  return `${Math.floor(minutes / 60)} sa`;
}

export const toolName = (tool: string) => (tool === 'claude' ? 'Claude' : tool === 'codex' ? 'Codex' : tool);

/** Ajanın Zody ailesindeki gövde rengi (araca göre). */
export const agentColor = (tool: string) => (tool === 'claude' ? '#f0977a' : tool === 'codex' ? '#5b9cff' : '#4fcf86');

/** "Terminale git" düğmesinin yazısı: oturumun penceresi hangi uygulamadaysa onun adı. */
export function windowLabel(app: string | null): string {
  const exe = (app ?? '').toLowerCase();
  if (exe === 'claude.exe') return "Claude'a git";
  if (exe === 'code.exe' || exe === 'code - insiders.exe') return "VS Code'a git";
  if (exe === 'cursor.exe') return "Cursor'a git";
  return 'Terminale git';
}

/** "+12 −3" (eksi işareti tipografik). */
export const lineCounts = (added: number, removed: number) => `+${added} −${removed}`;
