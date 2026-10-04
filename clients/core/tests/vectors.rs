use drop_core::{
    account_material, b64url_to_bytes, bytes_to_b64url, clipboard_file_kind, decode_item, decrypt,
    derive_keys, encode_item, encrypt, load_settings, registration_body, save_settings, verify_key_check,
    ClipboardKind, ItemKind, ItemPlain, KdfParams, KDF_MEMORY, KDF_PARALLELISM, KDF_TIME,
};

const VECTOR: &str = "4bd8ebfb202d2c08d9f467099ed686f4d05dc192458b76b0b4d925526abbed76607e940d53e0478f9bbffe3314fb22164beedacb4171bd29807fbbc54a760323";

fn params() -> KdfParams {
    KdfParams {
        algo: "argon2id".into(),
        memory: KDF_MEMORY,
        time: KDF_TIME,
        parallelism: KDF_PARALLELISM,
    }
}

#[test]
fn argon2id_split_matches_the_browser_vector() {
    let salt = vec![0x11u8; 16];
    let derived = derive_keys("drop-test-password", &salt, &params()).unwrap();
    let mut full = Vec::new();
    // Reconstruct the 64-byte Argon2 output: SHA-256(first half) is the verifier,
    // so compare the content key to the published second half and the verifier to SHA-256 of the first.
    let raw = hex_decode(VECTOR);
    assert_eq!(derived.content_key.as_slice(), &raw[32..]);
    let expected_verifier = sha256(&raw[..32]);
    assert_eq!(derived.auth_verifier.as_slice(), expected_verifier.as_slice());
    full.extend_from_slice(&raw);
    assert!(!derived.auth_verifier.as_slice().eq(derived.content_key.as_slice()));
    assert_eq!(hex_encode(&raw), VECTOR);
}

#[test]
fn passwords_are_normalized_before_derivation() {
    let salt = vec![0x22u8; 16];
    let latin = derive_keys("password1", &salt, &params()).unwrap();
    let fullwidth = derive_keys("password\u{FF11}", &salt, &params()).unwrap();
    let other = derive_keys("password2", &salt, &params()).unwrap();
    assert_eq!(latin.content_key.as_slice(), fullwidth.content_key.as_slice());
    assert_eq!(latin.auth_verifier.as_slice(), fullwidth.auth_verifier.as_slice());
    assert_ne!(latin.content_key.as_slice(), other.content_key.as_slice());
}

#[test]
fn weak_kdf_is_refused_before_derivation() {
    let salt = vec![0x11u8; 16];
    let mut weak = params();
    weak.memory = 1024;
    let error = match derive_keys("drop-test-password", &salt, &weak) {
        Err(error) => error,
        Ok(_) => panic!("weak kdf was accepted"),
    };
    assert!(error.to_string().to_lowercase().contains("weak") || error.to_string().contains("derivation"));
}

#[test]
fn aes_gcm_layout_matches_a_known_vector_and_rejects_tampering() {
    let key = hex_decode("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
    let mut key_arr = [0u8; 32];
    key_arr.copy_from_slice(&key);
    let iv = hex_decode("000102030405060708090a0b");
    let expected = hex_decode("2370b96be88ea762a022ffeed282551bb2c5e283ae92bb8b612327f9d0e65adde0");
    let blob = hex_decode("000102030405060708090a0b2370b96be88ea762a022ffeed282551bb2c5e283ae92bb8b612327f9d0e65adde0");
    let plain = decrypt(&key_arr, &blob).unwrap();
    assert_eq!(plain, b"drop-key-check-v1");
    assert_eq!(&blob[12..], expected.as_slice());
    assert_eq!(&blob[..12], iv.as_slice());

    let round = encrypt(&key_arr, b"meet at the north door").unwrap();
    assert_eq!(decrypt(&key_arr, &round).unwrap(), b"meet at the north door");
    let mut tampered = round.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0xff;
    assert!(decrypt(&key_arr, &tampered).is_err());

    let mut wrong = key_arr;
    wrong[0] ^= 0xff;
    assert!(decrypt(&wrong, &round).is_err());
}

#[test]
fn item_codec_matches_the_browser_layout() {
    let encoded = encode_item(&ItemPlain {
        kind: ItemKind::Text,
        name: String::new(),
        mime: "text/plain".into(),
        body: b"hi".to_vec(),
    })
    .unwrap();
    assert_eq!(hex_encode(&encoded), "44525031010000000a746578742f706c61696e6869");
    let decoded = decode_item(&encoded).unwrap();
    assert_eq!(decoded.kind, ItemKind::Text);
    assert_eq!(decoded.body, b"hi");

    let file = encode_item(&ItemPlain {
        kind: ItemKind::File,
        name: "notes/secret-plan.txt".into(),
        mime: "text/plain".into(),
        body: b"meet at the north door".to_vec(),
    })
    .unwrap();
    let back = decode_item(&file).unwrap();
    assert_eq!(back.name, "notes/secret-plan.txt");
    assert_eq!(back.body, b"meet at the north door");
}

#[test]
fn registration_payload_omits_the_password_and_content_key() {
    let password = "a-fine-password";
    let material = account_material(password).unwrap();
    let body = registration_body(&material);
    let keys: Vec<_> = body.as_object().unwrap().keys().cloned().collect();
    let mut keys = keys;
    keys.sort();
    assert_eq!(
        keys,
        vec!["authVerifier", "kdfMemory", "kdfParallelism", "kdfSalt", "kdfTime", "keyCheck"]
    );
    let json = body.to_string();
    assert!(!json.contains(password));
    assert!(!json.contains(&bytes_to_b64url(material.content_key.as_slice())));
    assert!(verify_key_check(&material.content_key, &material.key_check).unwrap());
    let _ = b64url_to_bytes(body["authVerifier"].as_str().unwrap()).unwrap();
}

#[test]
fn clipboard_kinds_match_the_browser() {
    assert_eq!(clipboard_file_kind("image/png"), Some(ClipboardKind::Image));
    assert_eq!(clipboard_file_kind("IMAGE/JPEG"), Some(ClipboardKind::Image));
    assert_eq!(clipboard_file_kind("text/plain; charset=utf-8"), Some(ClipboardKind::Text));
    assert_eq!(clipboard_file_kind("application/ld+json"), Some(ClipboardKind::Text));
    assert_eq!(clipboard_file_kind("image/svg+xml"), Some(ClipboardKind::Text));
    assert_eq!(clipboard_file_kind(" Text/HTML ; charset=UTF-8"), Some(ClipboardKind::Html));
    for mime in ["application/pdf", "application/zip", "application/octet-stream", "video/mp4", ""] {
        assert_eq!(clipboard_file_kind(mime), None, "{mime}");
    }
}

#[test]
fn settings_file_never_keeps_a_password_or_content_key() {
    let dir = std::env::temp_dir().join(format!("drop-settings-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.json");
    std::fs::write(
        &path,
        r#"{"serverUrl":"https://drop.example","username":"ada","password":"super-secret-password","contentKey":"not-a-real-key"}"#,
    )
    .unwrap();
    let settings = load_settings(&path);
    assert_eq!(settings.username, "ada");
    assert_eq!(settings.server_url, "https://drop.example");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("super-secret-password"));
    assert!(!text.contains("contentKey"));
    assert!(!text.contains("not-a-real-key"));
    save_settings(
        &path,
        &drop_core::Settings {
            server_url: "https://drop.example".into(),
            username: "ada".into(),
        },
    )
    .unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("password"));
    let missing = load_settings(&dir.join("missing.json"));
    assert_eq!(missing.server_url, "");
    assert_eq!(missing.username, "");
    assert!(!drop_core::DEFAULT_SERVER.contains("kiefermenard"));
    let _ = std::fs::remove_dir_all(&dir);
}

fn sha256(data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn hex_decode(value: &str) -> Vec<u8> {
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).unwrap())
        .collect()
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
