// Optimize derlenmiş zodyvel.exe'yi proje köküne Zodyvel.exe olarak kopyalar (çift tıkla açmak için).
// Yanına hook köprüsünü (zodyvel-hook.exe) de koyar: Zodyvel açılınca köprünün yeni sürümünü
// ~/.zodyvel/bin'e taşır (src-tauri/src/agents/hookbin.rs).
// Derleme klasörü .cargo/config.toml ile başka yere yönlendirilmiş olabilir; yeri cargo'ya sorulur.
// `--hook`: yalnız köprüyü kopyalar. Kurulum paketi köprüyü buradan alır (tauri.conf.json bundle.resources);
// beforeBuildCommand köprüyü derledikten hemen sonra bunu çalıştırır, paket bir önceki derlemenin köprüsünü almasın.
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const metadata = JSON.parse(
  execFileSync('cargo', ['metadata', '--format-version', '1', '--no-deps', '--manifest-path', join(root, 'src-tauri', 'Cargo.toml')], {
    encoding: 'utf8',
  }),
);
const source = join(metadata.target_directory, 'release', 'zodyvel.exe');
const target = join(root, 'Zodyvel.exe');
const hookSource = join(metadata.target_directory, 'release', 'zodyvel-hook.exe');
const hookTarget = join(root, 'zodyvel-hook.exe');

if (process.argv.includes('--hook')) {
  if (!existsSync(hookSource)) {
    console.error(`Bulunamadı: ${hookSource}`);
    process.exit(1);
  }
  copyFileSync(hookSource, hookTarget);
  process.exit(0);
}
if (!existsSync(source)) {
  console.error(`Bulunamadı: ${source}\nÖnce: pnpm tauri build --no-bundle`);
  process.exit(1);
}
try {
  copyFileSync(source, target);
  if (existsSync(hookSource)) copyFileSync(hookSource, hookTarget);
} catch (error) {
  // Zodyvel.exe açıkken üzerine yazılamaz.
  console.error(`Kopyalanamadı (Zodyvel.exe açık olabilir; tepsiden Çıkış yapıp tekrar dene): ${error.message}`);
  process.exit(1);
}
console.log(`Hazır: ${target}`);
