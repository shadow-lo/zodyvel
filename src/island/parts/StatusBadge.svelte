<script lang="ts">
  import type { AgentStatus } from '../../lib/types';

  const labels: Record<AgentStatus, string> = {
    running: 'Çalışıyor',
    waiting_input: 'Yanıt bekliyor',
    waiting_approval: 'Onay bekliyor',
    done: 'Tamamlandı',
    error: 'Hata',
  };

  // Durum çipi (Faz 1 §2.3): renkli zemin %13, parlayan nokta (durağan gölge, animasyon yok).
  // suffix: durumun yanındaki süre ("Çalışıyor · 4 dk").
  let { status, suffix = null }: { status: AgentStatus; suffix?: string | null } = $props();
</script>

<span class="badge {status}"><span class="dot"></span>{labels[status]}{#if suffix}{` · ${suffix}`}{/if}</span>

<style>
  .badge {
    --c: var(--text-2);
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 8px 2px 6px;
    border-radius: 999px;
    font-size: var(--fs-11);
    white-space: nowrap;
    color: var(--c);
    background: color-mix(in srgb, var(--c) 13%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--c) 18%, transparent);
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--c);
    box-shadow: 0 0 6px var(--c);
  }
  .running {
    --c: var(--teal);
  }
  .waiting_input {
    --c: var(--accent-soft);
  }
  .waiting_approval {
    --c: var(--amber);
  }
  .done {
    --c: var(--green);
  }
  .error {
    --c: var(--red);
  }
</style>
