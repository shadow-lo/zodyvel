import { describe, expect, it } from 'vitest';
import { demoAgents } from '../src/lib/demo';
import type { AgentSession } from '../src/lib/types';
import { firstTarget, folderFor, nextTaskNo, sendLabel, stripAgents, whenLabel, withAttachments, writeMode } from '../src/lib/write';

const base = demoAgents()[0]!;
const agent = (over: Partial<AgentSession>): AgentSession => ({ ...base, ...over });

describe('writeMode', () => {
  it('çalışan oturumda not sıraya girer; not alamıyorsa yazılmaz', () => {
    expect(writeMode(agent({ status: 'running', canNote: true }))).toBe('queue');
    expect(writeMode(agent({ status: 'waiting_approval', canNote: true }))).toBe('queue');
    expect(writeMode(agent({ status: 'running', canNote: false }))).toBe('blocked');
  });
  it('bitmiş oturum köprü bekliyorsa yeni tur, beklemiyorsa panoya kopya', () => {
    expect(writeMode(agent({ status: 'done', canTask: true }))).toBe('task');
    expect(writeMode(agent({ status: 'waiting_input', canTask: false }))).toBe('copy');
  });
  it("Codex'e adadan yazılmaz", () => {
    expect(writeMode(agent({ tool: 'codex', status: 'done', canNote: false, canTask: false }))).toBe('blocked');
  });
  it('düğme yazısı kipe göre', () => {
    expect(sendLabel('queue')).toBe('Sıraya koy');
    expect(sendLabel('copy')).toBe('Kopyala');
  });
});

describe('stripAgents', () => {
  const list = ['a', 'b', 'c', 'd', 'e'].map(id => agent({ id }));
  it('sığıyorsa hepsi', () => expect(stripAgents(list.slice(0, 3), null).more).toBe(0));
  it('seçili olan sonda olsa da görünür', () => {
    const r = stripAgents(list, 'e');
    expect(r.shown.map(a => a.id)).toEqual(['a', 'b', 'e']);
    expect(r.more).toBe(2);
  });
});

describe('firstTarget', () => {
  const open = agent({ id: 'o', status: 'running', canNote: true });
  const codex = agent({ id: 'x', tool: 'codex', status: 'running', canNote: false });
  it('istenen yazılabilirse o', () => expect(firstTarget([codex, open], 'o', true)).toBe('o'));
  it('istenen yazılamıyorsa yazılabilen ilk oturum', () => expect(firstTarget([codex, open], 'x', true)).toBe('o'));
  it('hiç yoksa yeni görev (özellik açıksa)', () => {
    expect(firstTarget([codex], null, true)).toBe('new');
    expect(firstTarget([codex], null, false)).toBeNull();
  });
});

describe('nextTaskNo', () => {
  it('açık görevlerin en büyüğünden bir fazla', () => {
    expect(nextTaskNo([agent({}), agent({ taskNo: 3 }), agent({ taskNo: 1 })])).toBe(4);
    expect(nextTaskNo([])).toBe(1);
  });
});

describe('whenLabel', () => {
  const now = new Date(2026, 9, 20, 19, 0).getTime();
  it('bugün saat, dün "Dün", eski tarih gün ay', () => {
    expect(whenLabel(new Date(2026, 9, 20, 9, 5).getTime(), now)).toBe('09:05');
    expect(whenLabel(new Date(2026, 9, 19, 22, 14).getTime(), now)).toBe('Dün 22:14');
    expect(whenLabel(new Date(2026, 9, 12, 8, 0).getTime(), now)).toBe('12 Eki');
  });
});

describe('ekler', () => {
  const file = { path: 'C:\\İş\\zodyvel\\src\\main.ts', name: 'main.ts', dir: false };
  it('ek yoksa metin aynen gider', () => expect(withAttachments('Bu ne?', [])).toBe('Bu ne?'));
  it('tek ek: yol tek satırda, klasör ayrı adla', () => {
    expect(withAttachments('Bunu açıkla', [file])).toBe('Bunu açıkla\n\nEkli dosya: C:\\İş\\zodyvel\\src\\main.ts');
    expect(withAttachments('Bak', [{ ...file, path: 'D:\\x', dir: true }])).toBe('Bak\n\nEkli klasör: D:\\x');
  });
  it('birden çok ek liste olur', () => {
    expect(withAttachments('Karşılaştır', [file, { ...file, path: 'D:\\b.txt' }])).toBe('Karşılaştır\n\nEkler:\n- C:\\İş\\zodyvel\\src\\main.ts\n- D:\\b.txt');
  });
  it('yeni görev klasörü: ekin içinde olduğu en derin son klasör', () => {
    const projects = [{ path: 'C:\\İş' }, { path: 'C:\\İş\\zodyvel\\' }, { path: 'C:\\İş\\zody' }];
    expect(folderFor(file.path, projects)).toBe('C:\\İş\\zodyvel\\');
    expect(folderFor('C:\\Users\\a\\x.ts', [{ path: 'c:\\users\\A' }])).toBe('c:\\users\\A');
    expect(folderFor('E:\\başka\\a.txt', projects)).toBeNull();
    expect(folderFor('C:/İş/zodyvel/src/x.ts', [{ path: 'C:\\İş\\zodyvel' }])).toBe('C:\\İş\\zodyvel');
  });
});
