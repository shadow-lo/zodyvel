//! O9 · Android Key Attestation doğrulaması (masaüstü, eşleştirmede).
//!
//! Telefon `hello` iletisinde imza anahtarının Keystore kanıt zincirini (`att`, hex DER, yaprak önce) yollar.
//! Burada denetlenen:
//! 1. Zincir 3–5 sertifika (en az bir, en çok `MAX_RV` ara sertifika); her sertifika bir sonrakinin anahtarıyla imzalı (ECDSA P-256/P-384, RSA PKCS#1 v1.5).
//!    İmzalayan her sertifika CA olmalı (`basicConstraints cA=TRUE`, `keyUsage` varsa `keyCertSign`) ve
//!    KeyDescription taşımamalı. Yoksa gerçek bir kanıtlı anahtarla imzalanmış sahte yaprak zincire eklenebilir.
//! 2. Son sertifikanın açık anahtarı Google'ın Key Attestation köklerinden biri (SPKI SHA-256 sabitlenmiş).
//! 3. Yaprağın açık anahtarı telefonun beyan ettiği `apub` ile aynı.
//! 4. Yapraktaki KeyDescription uzantısı: `attestationChallenge` = bu eşleştirmenin Noise el sıkışma özeti
//!    (zincir başka bir oturumdan taşınamaz), anahtar ve kanıt güvenlik seviyesi TEE ya da StrongBox,
//!    donanımın uyguladığı listede kullanıcı doğrulaması şart (`noAuthRequired` yok, `userAuthType` ≠ 0),
//!    anahtar telefonda üretilmiş olmalı (`origin` = GENERATED; dışarıdan aktarılanın kopyası olabilir).
//!
//! 5. G4: donanım listesindeki `rootOfTrust`: önyükleyici KİLİTLİ (`deviceLocked`) ve önyükleme `Verified` olmalı.
//!    Kilidi açık ya da değiştirilmiş sistemde Keystore yine kanıt üretir ama işletim sistemine güvenilmez.
//! 6. G4: Google iptal listesi (`android.googleapis.com/attestation/status`). Liste masaüstünde indirilir (telefondan
//!    hiçbir veri gitmez); zincirdeki ara sertifikaların seri numaraları listeye karşı denetlenir. Kayıtta yalnız
//!    seri numaralarının kısa özetleri tutulur (`rv_hash`), liste sonradan güncellenince yeniden denetlenir.
//! 7. M-D3: `attestationApplicationId` yalnız telefon uygulamasının paketini (`APP_ID`) içermeli.
//!
//! Bilerek yapılmayan (bkz. md/telefon-t4c.md): sertifika geçerlilik tarihleri (Android yaprak tarihleri
//! güvenilmez).
//! Doğrulanamayan telefon reddedilmez: "korumasız" işaretlenir ve kısıtlı çalışır (tasarım kararı).
//! Hata metinleri içerik/anahtar taşımaz.
use p256::ecdsa::signature::hazmat::PrehashVerifier;
use sha2::{Digest, Sha256, Sha384, Sha512};

/// Google Hardware Attestation kök anahtarları: SubjectPublicKeyInfo DER'inin SHA-256'sı.
/// Kaynak: https://android.googleapis.com/attestation/root (14.11.2026'da indirildi).
/// - RSA-4096 kök (2016'dan beri aynı anahtar; seri f92009e853b6b045 ve öncekiler).
/// - ECDSA P-384 kök "Key Attestation CA1" (2025–2035, RKP'nin yeni kökü).
pub const GOOGLE_ROOTS: [[u8; 32]; 2] = [
    hex32("feb2ea7551ee316ed4bb443c8293b884dbfdea40b603ee3e4f4a897e4580fbae"),
    hex32("3ee44512a1af2beb39c889490c60ea3f82e43f5d5a5532f5ab9419f676cd07ec"),
];

const fn hex32(s: &str) -> [u8; 32] {
    const fn nib(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            _ => panic!("hex"),
        }
    }
    let b = s.as_bytes();
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = (nib(b[2 * i]) << 4) | nib(b[2 * i + 1]);
        i += 1;
    }
    out
}

pub const MAX_CHAIN: usize = 10;
/// M-D3: telefon uygulamasının paket adı (`phone-app` → `applicationId`). Kanıttaki `attestationApplicationId`
/// yalnız bu paketi içermeli. İmza özeti denetlenmez: sabitlenecek yayın imzası henüz yapılandırılmadı.
pub const APP_ID: &str = "app.zodyvel.phone";
/// Tek sertifikanın en büyük boyutu (DER).
pub const MAX_CERT: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Tee,
    StrongBox,
}
impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Tee => "tee",
            Level::StrongBox => "strongbox",
        }
    }
}

/// Doğrulanmış zincirin sonucu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attested {
    pub level: Level,
    /// Ara sertifikaların (yaprak ve kök hariç) seri numarası özetleri (`rv_hash`), iptal denetimi için.
    pub rv: Vec<String>,
}

/// Kayıtta tutulan en çok seri özeti (Kimlik Bilgisi Yöneticisi blob sınırı 2560 bayt).
pub const MAX_RV: usize = 3;

/// Google kökleriyle doğrular.
pub fn verify(chain_hex: &[String], apub_hex: &str, challenge: &[u8]) -> Result<Level, &'static str> {
    verify_with(chain_hex, apub_hex, challenge, &GOOGLE_ROOTS)
}

/// Kökler parametreli (sınamalar kendi köklerini verir).
pub fn verify_with(chain_hex: &[String], apub_hex: &str, challenge: &[u8], roots: &[[u8; 32]]) -> Result<Level, &'static str> {
    verify_full(chain_hex, apub_hex, challenge, roots).map(|a| a.level)
}

/// Tam sonuç: seviye + iptal denetimi için ara sertifika seri özetleri.
pub fn verify_full(chain_hex: &[String], apub_hex: &str, challenge: &[u8], roots: &[[u8; 32]]) -> Result<Attested, &'static str> {
    if chain_hex.is_empty() {
        return Err("Kanıt zinciri yok (telefon desteklemiyor ya da emülatör).");
    }
    // M-O3: en az bir ara sertifika (iptal denetimi onların serisine bakar), en çok `MAX_RV` (hepsi kayda girer;
    // fazlası sessizce atlanmaz).
    if chain_hex.len() < 3 || chain_hex.len() > MAX_RV + 2 {
        return Err("Kanıt zinciri uzunluğu geçersiz.");
    }
    let ders: Vec<Vec<u8>> = chain_hex
        .iter()
        .map(|h| crate::pairing::hex_decode(h).filter(|d| !d.is_empty() && d.len() <= MAX_CERT))
        .collect::<Option<_>>()
        .ok_or("Kanıt zinciri okunamadı.")?;
    let certs: Vec<Cert> = ders.iter().map(|d| Cert::parse(d)).collect::<Option<_>>().ok_or("Kanıt sertifikası okunamadı.")?;

    // 1. İmza zinciri.
    for w in certs.windows(2) {
        if !verify_sig(w[1].spki, w[0].sig_alg, w[0].tbs, w[0].sig) {
            return Err("Kanıt zincirinde imza doğrulanamadı.");
        }
    }
    // 1b. İmzalayanlar CA olmalı; kanıt uzantısı yalnız yaprakta olabilir (M-K1).
    if certs[1..].iter().any(|c| c.attestation.is_some() || !c.ca || c.key_usage.is_some_and(|ku| ku & KU_KEY_CERT_SIGN == 0)) {
        return Err("Kanıt zincirinde yetkisiz ara sertifika var.");
    }
    // 2. Kök sabitlemesi.
    let root = certs.last().ok_or("Kanıt zinciri okunamadı.")?;
    let h: [u8; 32] = Sha256::digest(root.spki).into();
    if !roots.contains(&h) {
        return Err("Kanıt zinciri Google köküne bağlanmıyor.");
    }
    // 3. Yaprak anahtarı = apub.
    let leaf = &certs[0];
    let apub = crate::pairing::hex_decode(apub_hex).ok_or("İmza anahtarı geçersiz.")?;
    let claimed = p256::ecdsa::VerifyingKey::from_sec1_bytes(&apub).map_err(|_| "İmza anahtarı geçersiz.")?;
    let leaf_key = match parse_spki(leaf.spki) {
        Some(Key::P256(k)) => k,
        _ => return Err("Kanıtlanan anahtar beklenen türde değil."),
    };
    if leaf_key != claimed {
        return Err("Kanıtlanan anahtar telefonun anahtarıyla eşleşmiyor.");
    }
    // 4. KeyDescription.
    let ext = leaf.attestation.ok_or("Kanıt uzantısı yok.")?;
    let kd = KeyDescription::parse(ext).ok_or("Kanıt uzantısı okunamadı.")?;
    if kd.challenge != challenge {
        return Err("Kanıt bu eşleştirmeye ait değil.");
    }
    let level = match (kd.attestation_level, kd.keymint_level) {
        (2, 2) => Level::StrongBox,
        (a, k) if a >= 1 && k >= 1 && a <= 2 && k <= 2 => Level::Tee,
        _ => return Err("Anahtar donanımda değil (yazılım Keystore)."),
    };
    if kd.hw_no_auth_required || kd.hw_user_auth_type == 0 {
        return Err("Anahtar kullanıcı doğrulaması (parmak izi/PIN) istemiyor.");
    }
    // Köken donanım listesinde ve GENERATED (0) olmalı (M-Y1).
    if kd.hw_origin != Some(0) {
        return Err("Anahtar telefonda üretilmemiş (dışarıdan aktarılmış).");
    }
    // 5. Doğrulanmış önyükleme: kilitli önyükleyici + Verified (0). SelfSigned (özel ROM) da kabul edilmez.
    match kd.root_of_trust {
        None => return Err("Önyükleme bilgisi yok."),
        Some((locked, state)) if !locked || state != 0 => return Err("Telefonun önyüklemesi doğrulanmamış (kilidi açık ya da değiştirilmiş sistem)."),
        Some(_) => {}
    }
    // 6. M-D3: anahtarı bu uygulama üretmiş olmalı (aynı telefondaki başka uygulamanın kanıtı geçmez).
    if kd.app_ok != Some(true) {
        return Err("Kanıt bu uygulamaya ait değil.");
    }
    let rv = certs[1..certs.len() - 1].iter().map(|c| rv_hash(&serial_hex(c.serial))).collect();
    Ok(Attested { level, rv })
}

/// Seri numarası, Google listesindeki biçimde: küçük harf hex, baştaki sıfırlar atılmış.
pub fn serial_hex(serial: &[u8]) -> String {
    norm_serial(&crate::pairing::hex(serial))
}

fn norm_serial(s: &str) -> String {
    let t = s.trim().to_ascii_lowercase();
    let t = t.trim_start_matches('0');
    if t.is_empty() { "0".into() } else { t.to_owned() }
}

/// Seri numarasının kısa özeti (SHA-256'nın ilk 4 baytı, hex). Kayıtta yer tutmasın diye seri yerine bu saklanır.
pub fn rv_hash(serial_norm: &str) -> String {
    let h = Sha256::digest(serial_norm.as_bytes());
    crate::pairing::hex(&h[..4])
}

/// Google Key Attestation iptal listesi (yalnız iptal edilmiş seri numaralarının özetleri).
#[derive(Clone, Debug, Default)]
pub struct Revocations {
    hashes: std::collections::HashSet<String>,
}

/// İndirilen listenin en büyük boyutu.
pub const MAX_STATUS_BYTES: usize = 8 * 1024 * 1024;

impl Revocations {
    /// `{"entries": {"<seri hex>": {"status": "REVOKED", ...}, ...}}`. Biçim tutmazsa `None` (liste yok sayılır).
    pub fn from_json(json: &str) -> Option<Revocations> {
        if json.len() > MAX_STATUS_BYTES {
            return None;
        }
        let v: serde_json::Value = serde_json::from_str(json).ok()?;
        let entries = v.get("entries")?.as_object()?;
        // REVOKED ve SUSPENDED ikisi de güvenilmez sayılır (askıya alınan anahtar da şu an güvenilmez).
        let hashes = entries.keys().map(|k| rv_hash(&norm_serial(k))).collect();
        Some(Revocations { hashes })
    }
    pub fn len(&self) -> usize {
        self.hashes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.hashes.is_empty()
    }
    /// Özetlerden biri listede mi?
    pub fn revoked(&self, rv: &[String]) -> bool {
        rv.iter().any(|h| self.hashes.contains(h))
    }
}

// ---------------------------------------------------------------------------------------------
// En küçük DER okuyucu: yalnız kesin uzunluk (DER), çok baytlı etiket numarası desteklenir.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Tag {
    class: u8,
    constructed: bool,
    number: u32,
}
const fn univ(number: u32, constructed: bool) -> Tag {
    Tag { class: 0, constructed, number }
}
const SEQ: Tag = univ(16, true);
const INT: Tag = univ(2, false);
const ENUM: Tag = univ(10, false);
const OCTETS: Tag = univ(4, false);
const BITS: Tag = univ(3, false);
const OID: Tag = univ(6, false);

struct Der<'a>(&'a [u8]);

impl<'a> Der<'a> {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// (etiket, içerik, tüm TLV baytları)
    fn next(&mut self) -> Option<(Tag, &'a [u8], &'a [u8])> {
        let b = self.0;
        let first = *b.first()?;
        let mut i = 1;
        let mut number = u32::from(first & 0x1f);
        if number == 0x1f {
            number = 0;
            loop {
                let c = *b.get(i)?;
                i += 1;
                if number > (u32::MAX >> 7) {
                    return None;
                }
                number = (number << 7) | u32::from(c & 0x7f);
                if c & 0x80 == 0 {
                    break;
                }
            }
        }
        let l0 = *b.get(i)?;
        i += 1;
        let len = if l0 < 0x80 {
            usize::from(l0)
        } else {
            let n = usize::from(l0 & 0x7f);
            if n == 0 || n > 3 {
                return None; // belirsiz uzunluk ya da ≥ 16 MB
            }
            let mut l = 0usize;
            for _ in 0..n {
                l = (l << 8) | usize::from(*b.get(i)?);
                i += 1;
            }
            l
        };
        let end = i.checked_add(len)?;
        let content = b.get(i..end)?;
        let all = &b[..end];
        self.0 = &b[end..];
        Some((Tag { class: first >> 6, constructed: first & 0x20 != 0, number }, content, all))
    }
    fn expect(&mut self, t: Tag) -> Option<&'a [u8]> {
        let (tag, c, _) = self.next()?;
        (tag == t).then_some(c)
    }
    fn expect_all(&mut self, t: Tag) -> Option<&'a [u8]> {
        let (tag, _, all) = self.next()?;
        (tag == t).then_some(all)
    }
}

fn small_int(c: &[u8]) -> Option<i64> {
    if c.is_empty() || c.len() > 8 {
        return None;
    }
    let mut v: i64 = if c[0] & 0x80 != 0 { -1 } else { 0 };
    for b in c {
        v = (v << 8) | i64::from(*b);
    }
    Some(v)
}

// OID'ler (kodlanmış içerik baytları).
const OID_EC_PUBLIC_KEY: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01];
const OID_P256: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07];
const OID_P384: &[u8] = &[0x2b, 0x81, 0x04, 0x00, 0x22];
const OID_RSA: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01];
const OID_ECDSA_SHA256: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x02];
const OID_ECDSA_SHA384: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x03];
const OID_ECDSA_SHA512: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x04];
const OID_RSA_SHA256: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0b];
const OID_RSA_SHA384: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0c];
const OID_RSA_SHA512: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0d];
/// 2.5.29.19 · basicConstraints.
const OID_BASIC_CONSTRAINTS: &[u8] = &[0x55, 0x1d, 0x13];
/// 2.5.29.15 · keyUsage.
const OID_KEY_USAGE: &[u8] = &[0x55, 0x1d, 0x0f];
/// keyUsage ilk baytında keyCertSign (bit 5).
const KU_KEY_CERT_SIGN: u16 = 0x0400;
/// 1.3.6.1.4.1.11129.2.1.17 · Android KeyDescription.
const OID_KEY_DESCRIPTION: &[u8] = &[0x2b, 0x06, 0x01, 0x04, 0x01, 0xd6, 0x79, 0x02, 0x01, 0x11];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SigAlg {
    Ecdsa(Hash),
    Rsa(Hash),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hash {
    S256,
    S384,
    S512,
}

struct Cert<'a> {
    /// Seri numarası (INTEGER içeriği).
    serial: &'a [u8],
    /// İmzalanan TBSCertificate (tüm TLV).
    tbs: &'a [u8],
    sig_alg: SigAlg,
    sig: &'a [u8],
    /// SubjectPublicKeyInfo (tüm TLV).
    spki: &'a [u8],
    attestation: Option<&'a [u8]>,
    /// `basicConstraints cA` (uzantı yoksa `false`).
    ca: bool,
    /// `keyUsage` bitleri (ilk iki bayt, büyük uçlu); uzantı yoksa `None`.
    key_usage: Option<u16>,
}

fn alg_oid(c: &[u8]) -> Option<&[u8]> {
    Der(c).expect(OID)
}

impl<'a> Cert<'a> {
    fn parse(der: &'a [u8]) -> Option<Cert<'a>> {
        let mut top = Der(der);
        let cert = top.expect(SEQ)?;
        if !top.is_empty() {
            return None;
        }
        let mut c = Der(cert);
        let tbs_all = c.expect_all(SEQ)?;
        let sig_alg = match alg_oid(c.expect(SEQ)?)? {
            o if o == OID_ECDSA_SHA256 => SigAlg::Ecdsa(Hash::S256),
            o if o == OID_ECDSA_SHA384 => SigAlg::Ecdsa(Hash::S384),
            o if o == OID_ECDSA_SHA512 => SigAlg::Ecdsa(Hash::S512),
            o if o == OID_RSA_SHA256 => SigAlg::Rsa(Hash::S256),
            o if o == OID_RSA_SHA384 => SigAlg::Rsa(Hash::S384),
            o if o == OID_RSA_SHA512 => SigAlg::Rsa(Hash::S512),
            _ => return None,
        };
        let bits = c.expect(BITS)?;
        let (&unused, sig) = bits.split_first()?;
        if unused != 0 || !c.is_empty() {
            return None;
        }
        // TBSCertificate: [0] sürüm?, seri, imza alg., yayıncı, geçerlilik, özne, SPKI, [1]?, [2]?, [3] uzantılar?
        let mut t = Der(Der(tbs_all).expect(SEQ)?);
        let mut f = t.next()?;
        if f.0 == (Tag { class: 2, constructed: true, number: 0 }) {
            f = t.next()?;
        }
        if f.0 != INT {
            return None;
        }
        let serial = f.1;
        t.expect(SEQ)?; // imza alg. (dış alanla aynı olmalı, aşağıda karşılaştırılmaz; imza zaten dış alanla doğrulanır)
        t.expect(SEQ)?; // yayıncı
        t.expect(SEQ)?; // geçerlilik
        t.expect(SEQ)?; // özne
        let spki = t.expect_all(SEQ)?;
        let mut attestation = None;
        let mut ca = None;
        let mut key_usage = None;
        while let Some((tag, content, _)) = t.next() {
            if tag != (Tag { class: 2, constructed: true, number: 3 }) {
                continue;
            }
            let mut exts = Der(Der(content).expect(SEQ)?);
            while let Some((et, ec, _)) = exts.next() {
                if et != SEQ {
                    return None;
                }
                let mut e = Der(ec);
                let oid = e.expect(OID)?;
                let (mut vt, mut vc, _) = e.next()?;
                if vt == univ(1, false) {
                    // critical BOOLEAN
                    (vt, vc, _) = e.next()?;
                }
                if vt != OCTETS {
                    return None;
                }
                if oid == OID_KEY_DESCRIPTION {
                    if attestation.is_some() {
                        return None; // iki kez: belirsiz, reddet
                    }
                    attestation = Some(vc);
                } else if oid == OID_BASIC_CONSTRAINTS {
                    // SEQUENCE { cA BOOLEAN DEFAULT FALSE, pathLenConstraint INTEGER OPTIONAL }
                    if ca.is_some() {
                        return None;
                    }
                    let mut b = Der(Der(vc).expect(SEQ)?);
                    let mut is_ca = false;
                    if let Some((t, c, _)) = b.next() {
                        if t == BOOL {
                            is_ca = c.len() == 1 && c[0] != 0;
                        }
                    }
                    ca = Some(is_ca);
                } else if oid == OID_KEY_USAGE {
                    if key_usage.is_some() {
                        return None;
                    }
                    let bits = Der(vc).expect(BITS)?;
                    let (_, b) = bits.split_first()?;
                    let hi = u16::from(*b.first().unwrap_or(&0));
                    let lo = u16::from(*b.get(1).unwrap_or(&0));
                    key_usage = Some((hi << 8) | lo);
                }
            }
        }
        Some(Cert { serial, tbs: tbs_all, sig_alg, sig, spki, attestation, ca: ca.unwrap_or(false), key_usage })
    }
}

enum Key {
    P256(p256::ecdsa::VerifyingKey),
    P384(p384::ecdsa::VerifyingKey),
    Rsa(rsa::RsaPublicKey),
}

fn parse_spki(spki: &[u8]) -> Option<Key> {
    let mut s = Der(Der(spki).expect(SEQ)?);
    let mut alg = Der(s.expect(SEQ)?);
    let bits = s.expect(BITS)?;
    let (&unused, key) = bits.split_first()?;
    if unused != 0 {
        return None;
    }
    let oid = alg.expect(OID)?;
    if oid == OID_EC_PUBLIC_KEY {
        let curve = alg.expect(OID)?;
        if curve == OID_P256 {
            return p256::ecdsa::VerifyingKey::from_sec1_bytes(key).ok().map(Key::P256);
        }
        if curve == OID_P384 {
            return p384::ecdsa::VerifyingKey::from_sec1_bytes(key).ok().map(Key::P384);
        }
        return None;
    }
    if oid == OID_RSA {
        use rsa::pkcs8::DecodePublicKey;
        let k = rsa::RsaPublicKey::from_public_key_der(spki).ok()?;
        // Zayıf anahtar kabul edilmez.
        use rsa::traits::PublicKeyParts;
        return (k.size() >= 256).then_some(Key::Rsa(k));
    }
    None
}

fn digest(h: Hash, msg: &[u8]) -> Vec<u8> {
    match h {
        Hash::S256 => Sha256::digest(msg).to_vec(),
        Hash::S384 => Sha384::digest(msg).to_vec(),
        Hash::S512 => Sha512::digest(msg).to_vec(),
    }
}

fn verify_sig(issuer_spki: &[u8], alg: SigAlg, tbs: &[u8], sig: &[u8]) -> bool {
    let Some(key) = parse_spki(issuer_spki) else { return false };
    match (key, alg) {
        (Key::P256(k), SigAlg::Ecdsa(h)) => p256::ecdsa::Signature::from_der(sig).is_ok_and(|s| k.verify_prehash(&digest(h, tbs), &s).is_ok()),
        (Key::P384(k), SigAlg::Ecdsa(h)) => p384::ecdsa::Signature::from_der(sig).is_ok_and(|s| k.verify_prehash(&digest(h, tbs), &s).is_ok()),
        (Key::Rsa(k), SigAlg::Rsa(h)) => {
            use rsa::Pkcs1v15Sign;
            let scheme = match h {
                Hash::S256 => Pkcs1v15Sign::new::<Sha256>(),
                Hash::S384 => Pkcs1v15Sign::new::<Sha384>(),
                Hash::S512 => Pkcs1v15Sign::new::<Sha512>(),
            };
            k.verify(scheme, &digest(h, tbs), sig).is_ok()
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------------------------
// KeyDescription (Android Keystore/KeyMint şeması; sürümler 1–400 aynı ilk sekiz alan).

struct KeyDescription<'a> {
    attestation_level: i64,
    keymint_level: i64,
    challenge: &'a [u8],
    /// Donanımın uyguladığı listede `noAuthRequired` (503) var mı.
    hw_no_auth_required: bool,
    /// Donanımın uyguladığı listede `userAuthType` (504); yoksa 0.
    hw_user_auth_type: i64,
    /// Donanımın uyguladığı listede `origin` (702); yoksa `None`.
    hw_origin: Option<i64>,
    /// `rootOfTrust` (704): (deviceLocked, verifiedBootState). Yoksa `None`.
    root_of_trust: Option<(bool, i64)>,
    /// `attestationApplicationId` (709) yalnız `APP_ID` paketini mi içeriyor; alan yoksa `None`.
    app_ok: Option<bool>,
}

const TAG_NO_AUTH_REQUIRED: u32 = 503;
const TAG_USER_AUTH_TYPE: u32 = 504;
const TAG_ORIGIN: u32 = 702;
const TAG_ROOT_OF_TRUST: u32 = 704;
const TAG_APPLICATION_ID: u32 = 709;
const BOOL: Tag = univ(1, false);
const SET: Tag = univ(17, true);

/// AttestationApplicationId ::= SEQUENCE { packageInfos SET OF SEQUENCE { packageName OCTET STRING,
/// version INTEGER }, signatureDigests SET OF OCTET STRING } (OCTET STRING içinde). Okunamazsa `false`.
/// Paylaşılan kullanıcı kimliğinde birden çok paket olabilir: hepsi `APP_ID` olmalı.
fn app_id_ok(content: &[u8]) -> bool {
    let check = || -> Option<bool> {
        let inner = Der(content).expect(OCTETS)?;
        let mut s = Der(Der(inner).expect(SEQ)?);
        let mut infos = Der(s.expect(SET)?);
        let mut any = false;
        while let Some((t, c, _)) = infos.next() {
            if t != SEQ || Der(c).expect(OCTETS)? != APP_ID.as_bytes() {
                return Some(false);
            }
            any = true;
        }
        Some(any)
    };
    check() == Some(true)
}

impl<'a> KeyDescription<'a> {
    fn parse(ext: &'a [u8]) -> Option<KeyDescription<'a>> {
        let mut top = Der(ext);
        let mut s = Der(top.expect(SEQ)?);
        if !top.is_empty() {
            return None;
        }
        small_int(s.expect(INT)?)?; // attestationVersion
        let attestation_level = small_int(s.expect(ENUM)?)?;
        small_int(s.expect(INT)?)?; // keymaster/keyMint sürümü
        let keymint_level = small_int(s.expect(ENUM)?)?;
        let challenge = s.expect(OCTETS)?;
        s.expect(OCTETS)?; // uniqueId
        // softwareEnforced: yalnız `attestationApplicationId` okunur (Keystore'un, yani doğrulanmış önyüklemeli
        // işletim sisteminin yazdığı paket adı); diğer alanlar yazılımın beyanı, bilerek bakılmaz.
        let sw = s.expect(SEQ)?;
        let mut app_ok = None;
        let mut l = Der(sw);
        while let Some((tag, content, _)) = l.next() {
            if tag.class == 2 && tag.constructed && tag.number == TAG_APPLICATION_ID {
                app_ok = Some(app_id_ok(content));
            }
        }
        let hw = s.expect(SEQ)?;
        let mut hw_no_auth_required = false;
        let mut hw_user_auth_type = 0;
        let mut hw_origin = None;
        let mut root_of_trust = None;
        let mut l = Der(hw);
        while let Some((tag, content, _)) = l.next() {
            if tag.class != 2 || !tag.constructed {
                return None;
            }
            match tag.number {
                // Bazı Keymaster sürümleri bu alanı donanım listesine yazar; ikisi de aynı biçimde denetlenir.
                TAG_APPLICATION_ID => app_ok = Some(app_id_ok(content) && app_ok != Some(false)),
                TAG_NO_AUTH_REQUIRED => hw_no_auth_required = true,
                TAG_USER_AUTH_TYPE => hw_user_auth_type = small_int(Der(content).expect(INT)?)?,
                TAG_ORIGIN => hw_origin = Some(small_int(Der(content).expect(INT)?)?),
                TAG_ROOT_OF_TRUST => {
                    // RootOfTrust ::= SEQUENCE { verifiedBootKey OCTET STRING, deviceLocked BOOLEAN,
                    //                            verifiedBootState ENUMERATED, verifiedBootHash OCTET STRING (v3+) }
                    let mut r = Der(Der(content).expect(SEQ)?);
                    r.expect(OCTETS)?;
                    let locked = r.expect(BOOL)?;
                    let locked = locked.len() == 1 && locked[0] != 0;
                    let state = small_int(r.expect(ENUM)?)?;
                    root_of_trust = Some((locked, state));
                }
                _ => {}
            }
        }
        Some(KeyDescription { attestation_level, keymint_level, challenge, hw_no_auth_required, hw_user_auth_type, hw_origin, root_of_trust, app_ok })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn der_reads_high_tag_numbers() {
        // [504] EXPLICIT INTEGER 2  → bf 83 78 03 02 01 02
        let b = [0xbf, 0x83, 0x78, 0x03, 0x02, 0x01, 0x02];
        let (t, c, _) = Der(&b).next().unwrap();
        assert_eq!(t, Tag { class: 2, constructed: true, number: 504 });
        assert_eq!(small_int(Der(c).expect(INT).unwrap()), Some(2));
    }

    #[test]
    fn der_rejects_truncated_and_indefinite() {
        assert!(Der(&[0x30, 0x05, 0x01]).next().is_none());
        assert!(Der(&[0x30, 0x80, 0x00, 0x00]).next().is_none());
        assert!(Der(&[0xbf, 0x83]).next().is_none());
    }

    #[test]
    fn serials_are_normalised_like_googles_list() {
        assert_eq!(serial_hex(&[0x00, 0x0a, 0xbc]), "abc");
        assert_eq!(serial_hex(&[0x00]), "0");
        assert_eq!(norm_serial(" 00C35747A0 "), "c35747a0");
        assert_eq!(rv_hash("abc").len(), 8);
        let r = Revocations::from_json(r#"{"entries":{"0ABC":{"status":"REVOKED","reason":"KEY_COMPROMISE"},"def":{"status":"SUSPENDED"}}}"#).unwrap();
        assert!(r.revoked(&[rv_hash("abc")]));
        assert!(r.revoked(&[rv_hash("def")]), "askıya alınan da güvenilmez");
        assert!(!r.revoked(&[rv_hash("123")]));
        assert!(Revocations::from_json("{}").is_none());
        assert!(Revocations::from_json("bozuk").is_none());
    }

    #[test]
    fn roots_are_pinned() {
        assert_eq!(GOOGLE_ROOTS[0][0], 0xfe);
        assert_eq!(GOOGLE_ROOTS[1][31], 0xec);
    }
}
