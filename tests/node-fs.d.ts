// Testler Node'da çalışır; projede @types/node yok. Yalnız kullanılan çağrının imzası.
declare module 'node:fs' {
  export function readFileSync(path: URL | string, encoding: 'utf8'): string;
}
