#!/usr/bin/env node
// O9 sınama vektörleri: sahte kök (RSA-2048) → ara (P-384) → yaprak (P-256, KeyDescription uzantılı).
// Gerçek Google kökü kullanılmaz; sınama kendi kök özetini `verify_with`'e verir. Çıktı: Rust sabitleri.
// Çalıştırma (phone-proto klasöründen): node tests/attest_vectors.mjs > tests/attest_vectors.rs  (openssl gerekir)
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const run = (args, input) => execFileSync('openssl', args, { input, stdio: ['pipe', 'pipe', 'pipe'] });

const ln = (n) => {
  if (n < 0x80) return Buffer.from([n]);
  const b = [];
  for (let x = n; x > 0; x = Math.floor(x / 256)) b.unshift(x & 0xff);
  return Buffer.from([0x80 | b.length, ...b]);
};
const tlv = (t, c) => Buffer.concat([Buffer.from(t), ln(c.length), c]);
const ctx = (n, c) => {
  if (n < 31) return tlv([0xa0 | n], c);
  const ds = [];
  for (let x = n; ; ) {
    ds.unshift(x & 0x7f);
    x >>= 7;
    if (!x) break;
  }
  return tlv([0xbf, ...ds.slice(0, -1).map((d) => d | 0x80), ds[ds.length - 1]], c);
};
const integer = (v) => {
  const b = [];
  for (let x = v; x > 0; x >>= 8) b.unshift(x & 0xff);
  if (!b.length || b[0] & 0x80) b.unshift(0);
  return tlv([0x02], Buffer.from(b));
};
const enumv = (v) => tlv([0x0a], Buffer.from([v]));
const octets = (b) => tlv([0x04], b);
const seq = (...xs) => tlv([0x30], Buffer.concat(xs));
const setof = (...xs) => tlv([0x31], Buffer.concat(xs));
const boolean = (b) => tlv([0x01], Buffer.from([b ? 0xff : 0x00]));
const NULL = Buffer.from([0x05, 0x00]);

const CHALLENGE = Buffer.from([...Array(32).keys()]);
// M-D3: attestationApplicationId (709) = { SET OF { paket adı, sürüm }, SET OF imza özeti }.
const appId = (pkg) => octets(seq(setof(seq(octets(Buffer.from(pkg)), integer(1))), setof(octets(Buffer.alloc(32)))));
// origin (702): 0 = GENERATED, 2 = IMPORTED. null = alan yok. pkg: null = 709 alanı yok.
function keyDescription(att, km, { auth = true, challenge = CHALLENGE, rot = [true, 0], origin = 0, pkg = 'app.zodyvel.phone' } = {}) {
  const hw = [ctx(1, setof(integer(2))), ctx(2, integer(3))];
  hw.push(auth ? ctx(504, integer(3)) : ctx(503, NULL));
  if (origin !== null) hw.push(ctx(702, integer(origin)));
  // G4: rootOfTrust (704) = (verifiedBootKey, deviceLocked, verifiedBootState, verifiedBootHash)
  if (rot) hw.push(ctx(704, seq(octets(Buffer.alloc(32)), boolean(rot[0]), enumv(rot[1]), octets(Buffer.alloc(32)))));
  return seq(integer(200), enumv(att), integer(200), enumv(km), octets(challenge), octets(Buffer.alloc(0)),
    pkg === null ? seq() : seq(ctx(709, appId(pkg))), seq(...hw));
}

const d = mkdtempSync(join(tmpdir(), 'attv-'));
const p = (f) => join(d, f);
const ext = (name, text) => (writeFileSync(p(name), text), p(name));
run(['genpkey', '-algorithm', 'RSA', '-pkeyopt', 'rsa_keygen_bits:2048', '-out', p('root.key')]);
run(['req', '-x509', '-new', '-key', p('root.key'), '-subj', '/CN=test-root', '-days', '3650', '-sha256', '-out', p('root.pem')]);
run(['genpkey', '-algorithm', 'EC', '-pkeyopt', 'ec_paramgen_curve:P-384', '-out', p('int.key')]);
run(['req', '-new', '-key', p('int.key'), '-subj', '/CN=test-int', '-out', p('int.csr')]);
const inter = (name, cnf) => {
  const args = ['x509', '-req', '-in', p('int.csr'), '-CA', p('root.pem'), '-CAkey', p('root.key'), '-CAcreateserial', '-days', '3650', '-sha256', '-out', p(name + '.pem')];
  if (cnf !== null) args.push('-extfile', ext(name + '.cnf', cnf));
  run(args);
};
inter('int', 'basicConstraints=critical,CA:TRUE\nkeyUsage=critical,keyCertSign,cRLSign\n');
// M-K1: aynı ara anahtarın CA olmayan / imza yetkisiz sertifikaları.
inter('int_noca', null);
inter('int_ca_false', 'basicConstraints=critical,CA:FALSE\n');
inter('int_badku', 'basicConstraints=critical,CA:TRUE\nkeyUsage=critical,digitalSignature\n');
run(['genpkey', '-algorithm', 'EC', '-pkeyopt', 'ec_paramgen_curve:P-256', '-out', p('leaf.key')]);
run(['req', '-new', '-key', p('leaf.key'), '-subj', '/CN=Android Keystore Key', '-out', p('leaf.csr')]);
run(['genpkey', '-algorithm', 'EC', '-pkeyopt', 'ec_paramgen_curve:P-256', '-out', p('fake.key')]);
run(['req', '-new', '-key', p('fake.key'), '-subj', '/CN=Android Keystore Key', '-out', p('fake.csr')]);

const der = (pem) => run(['x509', '-in', pem, '-outform', 'DER']);
function sign(name, csr, ca, caKey, kd) {
  const args = ['x509', '-req', '-in', p(csr), '-CA', p(ca), '-CAkey', p(caKey), '-CAcreateserial', '-days', '3650', '-sha384', '-out', p(name + '.pem')];
  if (kd !== null) args.push('-extfile', ext(name + '.cnf', `1.3.6.1.4.1.11129.2.1.17=DER:${kd.toString('hex')}\n`));
  run(args);
  return der(p(name + '.pem'));
}
const leaf = (name, kd) => sign(name, 'leaf.csr', 'int.pem', 'int.key', kd);

const pubOf = (key) => run(['pkey', '-in', key, '-pubout', '-outform', 'DER']).subarray(-65);
const spki = run(['pkey', '-pubin', '-outform', 'DER'], run(['x509', '-in', p('root.pem'), '-pubkey', '-noout']));
const other = run(['pkey', '-pubout', '-outform', 'DER'], run(['genpkey', '-algorithm', 'EC', '-pkeyopt', 'ec_paramgen_curve:P-256'])).subarray(-65);

const out = {
  ROOT: der(p('root.pem')),
  INTER: der(p('int.pem')),
  INTER_NO_CA: der(p('int_noca.pem')),
  INTER_CA_FALSE: der(p('int_ca_false.pem')),
  INTER_BAD_KU: der(p('int_badku.pem')),
  LEAF_TEE: leaf('tee', keyDescription(1, 1)),
  LEAF_STRONGBOX: leaf('sb', keyDescription(2, 2)),
  LEAF_SOFTWARE: leaf('sw', keyDescription(0, 0)),
  LEAF_NO_AUTH: leaf('noauth', keyDescription(1, 1, { auth: false })),
  LEAF_NO_EXT: leaf('noext', null),
  LEAF_UNLOCKED: leaf('unlocked', keyDescription(1, 1, { rot: [false, 2] })),
  LEAF_SELF_SIGNED: leaf('selfsigned', keyDescription(1, 1, { rot: [true, 1] })),
  LEAF_NO_ROT: leaf('norot', keyDescription(1, 1, { rot: null })),
  // M-Y1: dışarıdan aktarılmış (IMPORTED) ya da kökeni yazmayan anahtar.
  LEAF_IMPORTED: leaf('imported', keyDescription(1, 1, { origin: 2 })),
  LEAF_NO_ORIGIN: leaf('noorigin', keyDescription(1, 1, { origin: null })),
  // M-D3: aynı telefondaki başka uygulamanın kanıtı / uygulama kimliği yok.
  LEAF_OTHER_APP: leaf('otherapp', keyDescription(1, 1, { pkg: 'com.example.other' })),
  LEAF_NO_APP: leaf('noapp', keyDescription(1, 1, { pkg: null })),
  // M-K1 saldırısı: gerçek kanıtlı yaprağın (LEAF_TEE) anahtarıyla imzalanmış sahte yaprak.
  // Zincir: FAKE_LEAF → LEAF_TEE → INTER → ROOT. Sahte yaprakta StrongBox ve başka bir anahtar yazar.
  FAKE_LEAF: sign('fake', 'fake.csr', 'tee.pem', 'leaf.key', keyDescription(2, 2)),
};
const interSerial = run(['x509', '-in', p('int.pem'), '-noout', '-serial']).toString().trim().split('=')[1].toLowerCase().replace(/^0+(?=.)/, '');
const lines = [
  '// Üretildi: tests/attest_vectors.mjs (sahte kök; gerçek Google kökü değil). Elle düzenlemeyin.',
  `pub const ROOT_PIN: &str = "${createHash('sha256').update(spki).digest('hex')}";`,
  `pub const APUB: &str = "${pubOf(p('leaf.key')).toString('hex')}";`,
  `pub const FAKE_APUB: &str = "${pubOf(p('fake.key')).toString('hex')}";`,
  `pub const OTHER_APUB: &str = "${other.toString('hex')}";`,
  `pub const CHALLENGE: &str = "${CHALLENGE.toString('hex')}";`,
  `pub const INTER_SERIAL: &str = "${interSerial}";`,
  ...Object.entries(out).map(([k, v]) => `pub const ${k}: &str = "${v.toString('hex')}";`),
];
process.stdout.write(lines.join('\n') + '\n');
