import { describe, expect, it } from 'vitest';
import { ACCESS_LEVELS, auditLine, type AuditEntry } from '../src/lib/actions';

describe('erişim seviyeleri', () => {
  it('Kapalı, Düşük, Orta sırayla gelir', () => {
    expect(ACCESS_LEVELS.map(l => l.id)).toEqual(['closed', 'low', 'mid']);
    expect(ACCESS_LEVELS.every(l => l.note.length > 10)).toBe(true);
  });

  it('denetim satırı onay yoksa onayı yazmaz', () => {
    const e: AuditEntry = { t: 1, zaman: '14:03:22', istek: 'müzik', seviye: 'low', eylem: 'music.control', ozet: '', onay: 'gerekmedi', onayMs: 0, sonuc: 'ok' };
    expect(auditLine(e)).toBe('14:03 · music.control · ok');
    expect(auditLine({ ...e, onay: 'kullanıcı' })).toBe('14:03 · music.control · onay: kullanıcı · ok');
  });
});
