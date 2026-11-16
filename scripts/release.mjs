// Yayın: `pnpm release 0.7.0 "Kısa sürüm notu"`
// Sürüm numarasını yazar, imzalı kurulum dosyasını derler, latest.json üretir; `gh` kuruluysa GitHub'a yükler.
import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO = 'shadow-lo/zodyvel-releases';
const [version, notesArg = '', notesFile] = process.argv.slice(2);
const notes = notesFile ? readFileSync(notesFile, 'utf8').trim() : notesArg;
if (!/^\d+\.\d+\.\d+$/.test(version ?? '')) {
  console.error('Kullanım: pnpm release 0.7.0 "sürüm notu"');
  process.exit(1);
}
const root = fileURLToPath(new URL('..', import.meta.url));
const key = process.env.TAURI_SIGNING_PRIVATE_KEY_PATH ?? join(homedir(), '.zodyvel', 'updater.key');
if (!existsSync(key)) {
  console.error(`İmza anahtarı yok (${key}). Önce: node scripts/updater-key.mjs`);
  process.exit(1);
}

// 1) Sürüm numarası: üç dosyada aynı olmalı.
const bump = (file, pattern) => {
  const path = join(root, file);
  const text = readFileSync(path, 'utf8');
  const next = text.replace(pattern, `$1${version}$2`);
  if (next === text && !text.includes(`"${version}"`)) throw new Error(`${file}: sürüm alanı bulunamadı`);
  writeFileSync(path, next);
};
bump('src-tauri/tauri.conf.json', /("version":\s*")[^"]+(")/);
bump('package.json', /("version":\s*")[^"]+(")/);
bump('src-tauri/Cargo.toml', /(^version\s*=\s*")[^"]+(")/m);

// 2) İmzalı derleme.
const env = { ...process.env, TAURI_SIGNING_PRIVATE_KEY: readFileSync(key, 'utf8'), TAURI_SIGNING_PRIVATE_KEY_PASSWORD: '' };
const build = spawnSync('pnpm', ['tauri', 'build'], { cwd: root, env, stdio: 'inherit', shell: true });
if (build.status !== 0) process.exit(build.status ?? 1);

// 3) Çıktılar.
const candidates = [];
if (process.env.CARGO_TARGET_DIR) candidates.push(join(process.env.CARGO_TARGET_DIR, 'release/bundle/nsis'));
try {
  const meta = JSON.parse(execFileSync('cargo', ['metadata', '--format-version', '1', '--no-deps'], { cwd: join(root, 'src-tauri'), encoding: 'utf8', maxBuffer: 1 << 28 }));
  candidates.push(join(meta.target_directory, 'release/bundle/nsis'));
} catch {}
candidates.push(join(root, 'src-tauri/target/release/bundle/nsis'), join(root, 'target/release/bundle/nsis'));
const dir = candidates.find(existsSync);
if (!dir) throw new Error(`nsis çıktı klasörü bulunamadı. Denenen yollar:\n${candidates.join('\n')}`);
const setup = readdirSync(dir).find(f => f.includes(`_${version}_`) && f.endsWith('-setup.exe'));
if (!setup || !existsSync(join(dir, `${setup}.sig`))) throw new Error('Kurulum dosyası ya da .sig bulunamadı');
const latest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms: {
    'windows-x86_64': {
      signature: readFileSync(join(dir, `${setup}.sig`), 'utf8').trim(),
      url: `https://github.com/${REPO}/releases/download/v${version}/${setup}`,
    },
  },
};
const latestPath = join(dir, 'latest.json');
writeFileSync(latestPath, JSON.stringify(latest, null, 2));
console.log(`Hazır: ${join(dir, setup)}\nHazır: ${latestPath}`);

// 4) GitHub'a yükle (gh varsa). latest.json en son yüklenir: yeni sürüm ancak dosya hazır olunca görünür.
const hasGh = spawnSync('gh', ['--version'], { shell: true }).status === 0;
if (!hasGh) {
  console.log(`gh kurulu değil. Elle: https://github.com/${REPO}/releases/new adresinde v${version} etiketiyle yayın aç, ` + `${setup} ve latest.json dosyalarını ekle.`);
  process.exit(0);
}
const notesPath = join(dir, 'notes.txt');
writeFileSync(notesPath, notes || `Zodyvel ${version}`);
execFileSync('gh', ['release', 'create', `v${version}`, join(dir, setup), join(dir, `${setup}.sig`), latestPath, '--repo', REPO, '--title', `Zodyvel ${version}`, '--notes-file', notesPath], { stdio: 'inherit' });
console.log('Yayınlandı.');
