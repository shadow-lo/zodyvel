<script lang="ts">
  import { onMount } from 'svelte';
  import { ipc } from './lib/ipc';
  import type { AppState } from './lib/types';
  import Welcome from './screens/Welcome.svelte';
  import Scan from './screens/Scan.svelte';
  import Code from './screens/Code.svelte';
  import Protect from './screens/Protect.svelte';
  import Shell from './screens/Shell.svelte';

  type Step = 'loading' | 'welcome' | 'scan' | 'code' | 'protect' | 'home';
  let step = $state<Step>('loading');
  let st = $state<AppState | null>(null);
  let fingerprint = $state('');
  let code = $state('');
  let notice = $state('');

  async function refresh(): Promise<void> {
    st = await ipc.state();
    step = st.phase === 'paired' ? 'home' : 'welcome';
  }
  onMount(() => {
    refresh().catch(() => {
      notice = 'Uygulama durumu okunamadı.';
      step = 'welcome';
    });
  });
</script>

{#if step === 'welcome'}
  <Welcome secure={st?.device_secure ?? true} {notice} onscan={() => { notice = ''; step = 'scan'; }} />
{:else if step === 'scan'}
  <Scan
    onback={() => (step = 'welcome')}
    onbegun={(b) => { code = b.code; fingerprint = b.fingerprint; step = 'code'; }}
    onfail={(m) => { notice = m; step = 'welcome'; }}
  />
{:else if step === 'code'}
  <Code {code} {fingerprint} oncancel={() => { ipc.pairCancel().catch(() => {}); step = 'welcome'; }} ondone={() => (step = 'protect')} onfail={(m) => { notice = m; step = 'welcome'; }} />
{:else if step === 'protect'}
  <Protect onfinish={() => refresh()} />
{:else if step === 'home' && st}
  <Shell {st} onchange={() => refresh()} />
{/if}
