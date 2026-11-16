/** "482916" → ["4","8","2","9","1","6"] (altı kutu). Geçersizse boş kutular. */
export function codeDigits(code: string): string[] {
  return /^\d{6}$/.test(code) ? code.split('') : Array(6).fill('·');
}

/** Rust'tan gelen hata metnini ekranda göstermek için güvenli kısaltır. */
export function errText(e: unknown): string {
  const s = typeof e === 'string' ? e : e instanceof Error ? e.message : '';
  return s.trim() ? s.slice(0, 200) : 'Bir şey ters gitti. Tekrar dene.';
}
