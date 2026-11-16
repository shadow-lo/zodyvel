//! O9: Key Attestation zinciri doğrulaması (sahte kökle üretilmiş vektörler, bkz. attest_vectors.mjs).
#![cfg(feature = "bridge")]
#[path = "attest_vectors.rs"]
mod v;

use zodyvel_phone_proto::attest::{rv_hash, verify, verify_full, verify_with, Level, Revocations};
use zodyvel_phone_proto::pairing::hex_decode;

fn pin() -> [u8; 32] {
    hex_decode(v::ROOT_PIN).unwrap().try_into().unwrap()
}
fn chain(leaf: &str) -> Vec<String> {
    vec![leaf.to_owned(), v::INTER.to_owned(), v::ROOT.to_owned()]
}
fn ch() -> Vec<u8> {
    hex_decode(v::CHALLENGE).unwrap()
}
fn check(leaf: &str) -> Result<Level, &'static str> {
    verify_with(&chain(leaf), v::APUB, &ch(), &[pin()])
}

#[test]
fn tee_and_strongbox_chains_pass() {
    assert_eq!(check(v::LEAF_TEE), Ok(Level::Tee));
    assert_eq!(check(v::LEAF_STRONGBOX), Ok(Level::StrongBox));
}

#[test]
fn software_key_or_missing_user_auth_is_unprotected() {
    assert!(check(v::LEAF_SOFTWARE).unwrap_err().contains("yazılım"));
    assert!(check(v::LEAF_NO_AUTH).unwrap_err().contains("kullanıcı doğrulaması"));
    assert!(check(v::LEAF_NO_EXT).unwrap_err().contains("uzantısı yok"));
}

#[test]
fn challenge_must_be_this_pairing() {
    let mut other = ch();
    other[0] ^= 1;
    assert!(verify_with(&chain(v::LEAF_TEE), v::APUB, &other, &[pin()]).unwrap_err().contains("bu eşleştirmeye"));
}

#[test]
fn leaf_key_must_match_apub() {
    assert!(verify_with(&chain(v::LEAF_TEE), v::OTHER_APUB, &ch(), &[pin()]).unwrap_err().contains("eşleşmiyor"));
}

#[test]
fn unknown_root_is_rejected() {
    assert!(verify_with(&chain(v::LEAF_TEE), v::APUB, &ch(), &[[0u8; 32]]).unwrap_err().contains("Google"));
    // Gerçek Google kökleriyle sahte zincir geçmez.
    assert!(verify(&chain(v::LEAF_TEE), v::APUB, &ch()).is_err());
}

#[test]
fn tampered_or_reordered_chain_is_rejected() {
    // Yapraktaki uzantı içinde bir bayt (imzalı alan) değişirse imza tutmaz.
    let mut leaf = hex_decode(v::LEAF_TEE).unwrap();
    let mid = leaf.len() / 2;
    leaf[mid] ^= 0x01;
    let tampered = zodyvel_phone_proto::pairing::hex(&leaf);
    assert!(check(&tampered).is_err());
    // Ara sertifika atlanırsa yaprak kökle doğrulanamaz.
    assert!(verify_with(&[v::LEAF_TEE.into(), v::ROOT.into()], v::APUB, &ch(), &[pin()]).is_err());
    // Sıra bozuk.
    assert!(verify_with(&[v::INTER.into(), v::LEAF_TEE.into(), v::ROOT.into()], v::APUB, &ch(), &[pin()]).is_err());
}

#[test]
fn empty_garbage_or_oversized_chains_are_unprotected_not_panics() {
    assert!(verify_with(&[], v::APUB, &ch(), &[pin()]).unwrap_err().contains("yok"));
    assert!(verify_with(&[v::LEAF_TEE.into()], v::APUB, &ch(), &[pin()]).is_err());
    assert!(verify_with(&["zz".into(), "00".into()], v::APUB, &ch(), &[pin()]).is_err());
    assert!(verify_with(&["3000".into(), "3003020100".into()], v::APUB, &ch(), &[pin()]).is_err());
    let long: Vec<String> = (0..11).map(|_| v::ROOT.to_owned()).collect();
    assert!(verify_with(&long, v::APUB, &ch(), &[pin()]).is_err());
    let huge = "30".repeat(9000);
    assert!(verify_with(&[huge.clone(), huge], v::APUB, &ch(), &[pin()]).is_err());
}

#[test]
fn g4_unlocked_or_modified_boot_is_unprotected() {
    assert!(check(v::LEAF_UNLOCKED).unwrap_err().contains("önyükleme"));
    assert!(check(v::LEAF_SELF_SIGNED).unwrap_err().contains("önyükleme"), "özel ROM (SelfSigned) da korumasız");
    assert!(check(v::LEAF_NO_ROT).unwrap_err().contains("Önyükleme bilgisi yok"));
}

#[test]
fn g4_intermediate_serials_are_checked_against_googles_list() {
    let a = verify_full(&chain(v::LEAF_TEE), v::APUB, &ch(), &[pin()]).unwrap();
    assert_eq!(a.level, Level::Tee);
    // Yalnız ara sertifika (yaprak ve kök hariç).
    assert_eq!(a.rv, vec![rv_hash(v::INTER_SERIAL)]);
    let listed = Revocations::from_json(&format!(r#"{{"entries":{{"{}":{{"status":"REVOKED","reason":"KEY_COMPROMISE"}}}}}}"#, v::INTER_SERIAL.to_uppercase())).unwrap();
    assert!(listed.revoked(&a.rv));
    let other = Revocations::from_json(r#"{"entries":{"1234":{"status":"REVOKED"}}}"#).unwrap();
    assert!(!other.revoked(&a.rv));
}

#[test]
fn g4_every_attestation_reason_has_a_short_code() {
    use zodyvel_phone_proto::devices::WHY_TEXTS;
    let mut errs = vec![
        check(v::LEAF_SOFTWARE).unwrap_err(), check(v::LEAF_NO_AUTH).unwrap_err(), check(v::LEAF_NO_EXT).unwrap_err(),
        check(v::LEAF_UNLOCKED).unwrap_err(), check(v::LEAF_NO_ROT).unwrap_err(),
        verify_with(&chain(v::LEAF_TEE), v::OTHER_APUB, &ch(), &[pin()]).unwrap_err(),
        verify_with(&chain(v::LEAF_TEE), v::APUB, &ch(), &[[0u8; 32]]).unwrap_err(),
        verify_with(&[], v::APUB, &ch(), &[pin()]).unwrap_err(),
        verify_with(&[v::LEAF_TEE.into()], v::APUB, &ch(), &[pin()]).unwrap_err(),
    ];
    let mut other = ch();
    other[0] ^= 1;
    errs.push(verify_with(&chain(v::LEAF_TEE), v::APUB, &other, &[pin()]).unwrap_err());
    for e in errs {
        assert!(WHY_TEXTS.contains(&e), "kodsuz neden: {e}");
    }
}

#[test]
fn k1_leaf_signed_by_attested_key_is_rejected() {
    // Saldırı: gerçek kanıtlı anahtarla (LEAF_TEE) imzalanmış sahte StrongBox yaprağı zincire eklenir.
    let fake = vec![v::FAKE_LEAF.to_owned(), v::LEAF_TEE.to_owned(), v::INTER.to_owned(), v::ROOT.to_owned()];
    assert_eq!(verify_with(&fake, v::FAKE_APUB, &ch(), &[pin()]), Err("Kanıt zincirinde yetkisiz ara sertifika var."));
}

#[test]
fn k1_intermediate_must_be_a_ca_with_cert_sign() {
    for inter in [v::INTER_NO_CA, v::INTER_CA_FALSE, v::INTER_BAD_KU] {
        let c = vec![v::LEAF_TEE.to_owned(), inter.to_owned(), v::ROOT.to_owned()];
        assert_eq!(verify_with(&c, v::APUB, &ch(), &[pin()]), Err("Kanıt zincirinde yetkisiz ara sertifika var."));
    }
}

#[test]
fn y1_imported_or_unknown_origin_is_unprotected() {
    assert!(check(v::LEAF_IMPORTED).unwrap_err().contains("aktarılmış"));
    assert!(check(v::LEAF_NO_ORIGIN).unwrap_err().contains("aktarılmış"));
}

#[test]
fn k1_y1_reasons_have_short_codes() {
    use zodyvel_phone_proto::devices::WHY_TEXTS;
    let fake = vec![v::FAKE_LEAF.to_owned(), v::LEAF_TEE.to_owned(), v::INTER.to_owned(), v::ROOT.to_owned()];
    for e in [verify_with(&fake, v::FAKE_APUB, &ch(), &[pin()]).unwrap_err(), check(v::LEAF_IMPORTED).unwrap_err()] {
        assert!(WHY_TEXTS.contains(&e), "kodsuz neden: {e}");
    }
}

#[test]
fn d3_attestation_of_another_app_is_unprotected() {
    use zodyvel_phone_proto::devices::WHY_TEXTS;
    for leaf in [v::LEAF_OTHER_APP, v::LEAF_NO_APP] {
        let e = check(leaf).unwrap_err();
        assert_eq!(e, "Kanıt bu uygulamaya ait değil.");
        assert!(WHY_TEXTS.contains(&e), "kodsuz neden: {e}");
    }
}

#[test]
fn o3_chain_needs_an_intermediate_and_at_most_max_rv() {
    use zodyvel_phone_proto::attest::MAX_RV;
    // Ara sertifikasız zincirde iptal denetimi yapılamaz.
    assert_eq!(verify_with(&[v::LEAF_TEE.into(), v::ROOT.into()], v::APUB, &ch(), &[pin()]), Err("Kanıt zinciri uzunluğu geçersiz."));
    // MAX_RV'den çok ara sertifika sessizce kırpılmaz, reddedilir.
    let mut long = vec![v::LEAF_TEE.to_owned()];
    long.extend((0..=MAX_RV).map(|_| v::INTER.to_owned()));
    long.push(v::ROOT.to_owned());
    assert_eq!(verify_with(&long, v::APUB, &ch(), &[pin()]), Err("Kanıt zinciri uzunluğu geçersiz."));
}
