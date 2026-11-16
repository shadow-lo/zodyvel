// Sekme görünümlerinin tembel yükleyicileri (Faz 2b sekme kaydının bileşen yarısı; veri yarısı lib/tabs.ts).
// Her görünüm ayrı bir parça (chunk) olarak paketlenir ve ancak sekmesi görünür olduğunda yüklenir:
// kapalı modülün ekranı hiç indirilmez, derlenmez, RAM tutmaz. `TAB_VIEWS` `Record<Tab, …>` olduğundan
// kayda eklenen yeni bir sekme burada yükleyicisi yazılmadan derlenmez.

import type { Component } from 'svelte';
import type { Tab } from '../lib/tabs';

type Loader = () => Promise<{ default: Component<any> }>;

export const VIEWS = {
  miniAI: () => import('./views/MiniAI.svelte'),
  detailAI: () => import('./views/DetailAI.svelte'),
  github: () => import('./views/DetailGithub.svelte'),
  write: () => import('./views/WriteScene.svelte'),
  clock: () => import('./views/ClockHome.svelte'),
  music: () => import('./views/DetailMusic.svelte'),
  today: () => import('./views/DetailToday.svelte'),
  agenda: () => import('./views/AgendaView.svelte'),
  modes: () => import('./views/ModesView.svelte'),
  mail: () => import('./views/MailView.svelte'),
  pc: () => import('./views/SystemView.svelte'),
  shelf: () => import('./views/ShelfView.svelte'),
  zody: () => import('./views/ZodyChat.svelte'),
} satisfies Record<string, Loader>;

export type ViewKey = keyof typeof VIEWS;

/** Sekme görünürken ısıtılacak görünümler. GitHub ayrıntısı ve yazma sahnesi istenince yüklenir. */
export const TAB_VIEWS = {
  clock: ['clock'],
  ai: ['miniAI', 'detailAI'],
  music: ['music'],
  today: ['today'],
  agenda: ['agenda'],
  modes: ['modes'],
  mail: ['mail'],
  pc: ['pc'],
  shelf: ['shelf'],
  zody: ['zody'],
} satisfies Record<Tab, ViewKey[]>;
