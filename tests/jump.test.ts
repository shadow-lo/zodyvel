import { describe, expect, it } from 'vitest';
import { demoAgents, demoApproval } from '../src/lib/demo';
import { approvalJump, terminalTarget } from '../src/lib/jump';
import type { AgentSession } from '../src/lib/types';

const base = demoAgents()[0]!;
const agent = (over: Partial<AgentSession>): AgentSession => ({ ...base, app: 'WindowsTerminal.exe', status: 'running', statusSince: 0, lastSeen: 0, ...over });
const ask = (session: AgentSession, id: string) => ({ ...demoApproval(session), id });

describe('approvalJump', () => {
  const a = agent({ id: 'a' });
  it('görünen ilk istek, katlama kalkmaz', () => expect(approvalJump([ask(a, 'r1'), ask(a, 'r2')], [ask(a, 'r3')])).toEqual({ id: 'r1', unfold: false }));
  it('yalnız katlanmış istek varsa o açılır', () => expect(approvalJump([], [ask(a, 'r3')])).toEqual({ id: 'r3', unfold: true }));
  it('bekleyen istek yoksa null', () => expect(approvalJump([], [])).toBeNull());
});

describe('terminalTarget', () => {
  const running = agent({ id: 'run', lastSeen: 50 });
  const waiting = agent({ id: 'wait', status: 'waiting_input', statusSince: 10 });
  const later = agent({ id: 'later', status: 'waiting_input', statusSince: 20 });
  const broken = agent({ id: 'err', status: 'error' });
  const hidden = agent({ id: 'hid', status: 'waiting_approval', app: null });

  it('onay bekleyen oturum önce; penceresi bilinmeyen atlanır', () => {
    expect(terminalTarget([running, waiting], [ask(hidden, 'h'), ask(running, 'r')], null)?.id).toBe('run');
    expect(terminalTarget([hidden, waiting], [ask(hidden, 'h')], null)?.id).toBe('wait');
  });
  it('istek yoksa en son seni bekleyen, sonra hata veren', () => {
    expect(terminalTarget([running, waiting, later, broken], [], null)?.id).toBe('later');
    expect(terminalTarget([running, broken], [], null)?.id).toBe('err');
  });
  it('kimse beklemiyorsa seçili, o da yoksa en son olay gelen', () => {
    const old = agent({ id: 'old', lastSeen: 5 });
    expect(terminalTarget([running, old], [], 'old')?.id).toBe('old');
    expect(terminalTarget([old, running], [], null)?.id).toBe('run');
  });
  it('penceresi bilinen oturum yoksa null', () => expect(terminalTarget([hidden], [ask(hidden, 'h')], null)).toBeNull());
});
