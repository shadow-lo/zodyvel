// Planlar için küçük, güvenli Markdown okuyucu. HTML üretmez: bileşen blokları ve satır içi parçaları
// düz metin olarak çizer ({@html} yok), bu yüzden planın içindeki etiketler yalnız yazı olarak görünür.
// Desteklenen: başlık, paragraf, madde/numaralı liste (girintili, [ ] / [x]), kod bloğu, alıntı,
// tablo, yatay çizgi; satır içinde `kod`, **kalın**, *eğik*, [bağlantı](adres) → yalnız metni.

export type Inline = { t: 'text' | 'code' | 'strong' | 'em'; v: string };

export interface ListItem {
  depth: number;
  /** null: görev kutusu yok · true/false: [x] / [ ] */
  check: boolean | null;
  text: Inline[];
}

export type Block =
  | { t: 'h'; level: 1 | 2 | 3; text: Inline[] }
  | { t: 'p'; text: Inline[] }
  | { t: 'list'; ordered: boolean; start: number; items: ListItem[] }
  | { t: 'code'; lang: string; text: string }
  | { t: 'quote'; text: Inline[] }
  | { t: 'table'; head: Inline[][]; rows: Inline[][][] }
  | { t: 'hr' };

const FENCE = /^\s*(```|~~~)\s*([\w+-]*)/;
const HEADING = /^(#{1,6})\s+(.*?)(?:\s+#+)?\s*$/;
const ITEM = /^(\s*)([-*+]|\d{1,4}[.)])\s+(.*)$/;
const RULE = /^\s*([-*_])(\s*\1){2,}\s*$/;
const TABLE_SEP = /^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$/;

export function parseMarkdown(source: string): Block[] {
  const lines = source.replace(/\r\n?/g, '\n').split('\n');
  const at = (n: number): string => lines[n] ?? '';
  const blocks: Block[] = [];
  let para: string[] = [];
  const flush = () => {
    if (para.length) blocks.push({ t: 'p', text: inline(para.join(' ')) });
    para = [];
  };

  for (let i = 0; i < lines.length; i++) {
    const line = at(i);
    const fence = FENCE.exec(line);
    if (fence) {
      flush();
      const close = fence[1] ?? '```';
      const body: string[] = [];
      i++;
      while (i < lines.length && !at(i).trimStart().startsWith(close)) body.push(at(i++));
      blocks.push({ t: 'code', lang: fence[2] ?? '', text: body.join('\n') });
      continue;
    }
    if (!line.trim()) {
      flush();
      continue;
    }
    if (RULE.test(line)) {
      flush();
      blocks.push({ t: 'hr' });
      continue;
    }
    const heading = HEADING.exec(line);
    if (heading) {
      flush();
      const level = Math.min(3, (heading[1] ?? '#').length) as 1 | 2 | 3;
      blocks.push({ t: 'h', level, text: inline(heading[2] ?? '') });
      continue;
    }
    if (line.trimStart().startsWith('|') && i + 1 < lines.length && TABLE_SEP.test(at(i + 1))) {
      flush();
      const head = cells(line);
      const rows: Inline[][][] = [];
      i += 2;
      while (i < lines.length && at(i).trimStart().startsWith('|')) rows.push(cells(at(i++)));
      i--;
      blocks.push({ t: 'table', head, rows });
      continue;
    }
    if (line.trimStart().startsWith('>')) {
      flush();
      const body: string[] = [];
      while (i < lines.length && at(i).trimStart().startsWith('>')) body.push(at(i++).trimStart().replace(/^>\s?/, ''));
      i--;
      blocks.push({ t: 'quote', text: inline(body.join(' ')) });
      continue;
    }
    const item = ITEM.exec(line);
    if (item) {
      flush();
      const marker = item[2] ?? '-';
      const ordered = /\d/.test(marker);
      const items: ListItem[] = [];
      const base = indentOf(item[1]);
      while (i < lines.length) {
        const m = ITEM.exec(at(i));
        const last = items[items.length - 1];
        if (m) {
          let text = m[3] ?? '';
          let check: boolean | null = null;
          const box = /^\[([ xX])\]\s+/.exec(text);
          if (box) {
            check = box[1] !== ' ';
            text = text.slice(box[0].length);
          }
          items.push({ depth: Math.min(3, Math.max(0, Math.floor((indentOf(m[1]) - base) / 2))), check, text: inline(text) });
          i++;
        } else if (last && at(i).trim() && /^\s{2,}/.test(at(i))) {
          // Maddenin alt satırı: aynı maddeye eklenir.
          last.text = [...last.text, { t: 'text', v: ' ' }, ...inline(at(i).trim())];
          i++;
        } else break;
      }
      i--;
      blocks.push({ t: 'list', ordered, start: ordered ? parseInt(marker, 10) : 1, items });
      continue;
    }
    para.push(line.trim());
  }
  flush();
  return blocks;
}

function indentOf(space: string | undefined): number {
  return (space ?? '').replace(/\t/g, '  ').length;
}

function cells(line: string): Inline[][] {
  return line
    .trim()
    .replace(/^\|/, '')
    .replace(/\|$/, '')
    .split('|')
    .map(cell => inline(cell.trim()));
}

/** Satır içi biçim. İç içe biçim düzleştirilir (kalın içindeki eğik düz kalın olur). */
export function inline(text: string): Inline[] {
  const out: Inline[] = [];
  const push = (t: Inline['t'], v: string) => {
    if (!v) return;
    const last = out[out.length - 1];
    if (last && last.t === t && t === 'text') last.v += v;
    else out.push({ t, v });
  };
  let i = 0;
  while (i < text.length) {
    const c = text.charAt(i);
    if (c === '\\' && i + 1 < text.length && /[\\`*_[\]()#|>-]/.test(text.charAt(i + 1))) {
      push('text', text.charAt(i + 1));
      i += 2;
      continue;
    }
    if (c === '`') {
      const end = text.indexOf('`', i + 1);
      if (end > i) {
        push('code', text.slice(i + 1, end));
        i = end + 1;
        continue;
      }
    }
    if ((c === '*' || c === '_') && text.charAt(i + 1) === c) {
      const end = text.indexOf(c + c, i + 2);
      if (end > i + 2) {
        push('strong', strip(text.slice(i + 2, end)));
        i = end + 2;
        continue;
      }
    }
    if ((c === '*' || c === '_') && text.charAt(i + 1) !== c && text.charAt(i + 1) !== ' ') {
      // Kelime içindeki alt çizgi (dosya_adı) biçim değildir.
      const wordy = c === '_' && i > 0 && /\w/.test(text.charAt(i - 1));
      const end = text.indexOf(c, i + 1);
      if (!wordy && end > i + 1 && text.charAt(end - 1) !== ' ') {
        push('em', strip(text.slice(i + 1, end)));
        i = end + 1;
        continue;
      }
    }
    if (c === '[') {
      const m = /^\[([^\]]+)\]\(([^)\s]*)\)/.exec(text.slice(i));
      if (m) {
        push('text', strip(m[1] ?? ''));
        i += m[0].length;
        continue;
      }
    }
    push('text', c);
    i++;
  }
  return out;
}

function strip(text: string): string {
  return text.replace(/[`*_]/g, '');
}
