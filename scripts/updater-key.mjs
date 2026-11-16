// Bir kez çalıştırılır: güncelleme imza anahtarını bu bilgisayarda üretir ve genel anahtarı tauri.conf.json'a yazar.
// Özel anahtar %USERPROFILE%\.zodyvel\updater.key içinde kalır; depoya GİRMEZ, kimseye gönderilmez. Kaybolursa eski
// kurulumlar yeni güncellemeleri doğrulayamaz; yedeğini güvenli bir yerde tut.
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const dir = join(homedir(), '.zodyvel');
const key = join(dir, 'updater.key');
const conf = fileURLToPath(new URL('../src-tauri/tauri.conf.json', import.meta.url));

if (existsSync(key)) {
  console.log(`Anahtar zaten var: ${key} (üzerine yazılmadı).`);
} else {
  mkdirSync(dir, { recursive: true });
  execFileSync('pnpm', ['tauri', 'signer', 'generate', '--ci', '-w', key], { stdio: 'inherit', shell: true });
}

const pub = readFileSync(`${key}.pub`, 'utf8').trim();
const text = readFileSync(conf, 'utf8');
const next = text.replace(/("pubkey":\s*")[^"]*(")/, `$1${pub}$2`);
if (next === text && !text.includes(pub)) throw new Error('tauri.conf.json içinde "pubkey" alanı bulunamadı.');
writeFileSync(conf, next);
console.log('Genel anahtar tauri.conf.json içine yazıldı.');
