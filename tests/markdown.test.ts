import { describe, expect, it } from 'vitest';
import { inline, parseMarkdown, type Block } from '../src/lib/markdown';

function pick<T extends Block['t']>(block: Block | undefined, t: T): Extract<Block, { t: T }> {
  if (!block || block.t !== t) throw new Error(`${t} bekleniyordu`);
  return block as Extract<Block, { t: T }>;
}

describe('plan Markdown okuyucu', () => {
  it('başlık, liste, kod, alıntı, tablo ve çizgi', () => {
    const blocks = parseMarkdown(
      [
        '# Plan ##',
        '',
        'Giriş **kalın** ve `kod`.',
        'devamı',
        '',
        '1. bir',
        '2. iki',
        '   - alt [ayrıntı](https://x.y)',
        '',
        '- [x] bitti',
        '- [ ] kaldı',
        '',
        '```ts',
        'const a = 1;',
        '```',
        '> not',
        '---',
        '| A | B |',
        '|---|:-:|',
        '| 1 | `2` |',
      ].join('\n'),
    );
    expect(blocks.map(b => b.t)).toEqual(['h', 'p', 'list', 'list', 'code', 'quote', 'hr', 'table']);
    expect(blocks[0]).toEqual({ t: 'h', level: 1, text: [{ t: 'text', v: 'Plan' }] });
    expect(blocks[1]).toEqual({
      t: 'p',
      text: [
        { t: 'text', v: 'Giriş ' },
        { t: 'strong', v: 'kalın' },
        { t: 'text', v: ' ve ' },
        { t: 'code', v: 'kod' },
        { t: 'text', v: '. devamı' },
      ],
    });
    const ordered = pick(blocks[2], 'list');
    expect(ordered.ordered).toBe(true);
    expect(ordered.items.map(i => [i.depth, i.text.map(p => p.v).join('')])).toEqual([
      [0, 'bir'],
      [0, 'iki'],
      [1, 'alt ayrıntı'],
    ]);
    expect(pick(blocks[3], 'list').items.map(i => i.check)).toEqual([true, false]);
    expect(blocks[4]).toEqual({ t: 'code', lang: 'ts', text: 'const a = 1;' });
    const table = pick(blocks[7], 'table');
    expect([table.head.length, table.rows[0]?.[1]]).toEqual([2, [{ t: 'code', v: '2' }]]);
  });

  it('HTML yazı olarak kalır, kelime içindeki alt çizgi biçim değildir', () => {
    expect(inline('<img src=x onerror=alert(1)>')).toEqual([{ t: 'text', v: '<img src=x onerror=alert(1)>' }]);
    expect(inline('dosya_adi_uzun.rs')).toEqual([{ t: 'text', v: 'dosya_adi_uzun.rs' }]);
    expect(inline('C# ve *eğik* 2 * 3')).toEqual([
      { t: 'text', v: 'C# ve ' },
      { t: 'em', v: 'eğik' },
      { t: 'text', v: ' 2 * 3' },
    ]);
    expect(parseMarkdown('# C#')[0]).toEqual({ t: 'h', level: 1, text: [{ t: 'text', v: 'C#' }] });
  });

  it('kapanmayan kod bloğu sona kadar sürer, boş metin boş', () => {
    expect(parseMarkdown('```\nx\ny')).toEqual([{ t: 'code', lang: '', text: 'x\ny' }]);
    expect(parseMarkdown('  \n\n')).toEqual([]);
  });
});
