// Tembel görünüm önbelleği: `view(key)` yüklüyse bileşeni verir, değilse yüklemeyi başlatıp undefined
// döner (yüklenince ekran kendiliğinden çizilir). Bir kez yüklenen parça uygulama boyunca kalır.

import type { Component } from 'svelte';
import { VIEWS, type ViewKey } from './tab-views';

let loaded = $state.raw<Partial<Record<ViewKey, Component<any>>>>({});
const pending = new Map<ViewKey, Promise<void>>();

export function load(key: ViewKey): Promise<void> {
  let job = pending.get(key);
  if (!job) {
    job = VIEWS[key]()
      .then(mod => {
        loaded = { ...loaded, [key]: mod.default };
      })
      .catch(() => {
        // Parça okunamadı: bir sonraki istekte yeniden denenir.
        pending.delete(key);
      });
    pending.set(key, job);
  }
  return job;
}

export function view(key: ViewKey): Component<any> | undefined {
  const comp = loaded[key];
  if (!comp) void load(key);
  return comp;
}
