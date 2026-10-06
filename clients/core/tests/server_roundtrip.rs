use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use drop_core::{
    account_material, bytes_to_b64url, decrypt, decode_item, encode_item, encrypt, registration_body, DropClient,
    ItemKind, ItemPlain,
};

struct Server {
    child: Child,
    url: String,
    data_dir: PathBuf,
    secret: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.data_dir);
    }
}

#[test]
fn native_client_matches_the_server_and_the_browser_crypto() {
    let server = start_server();
    let password = "a-fine-password";
    let material = account_material(password).unwrap();
    let mut body = registration_body(&material);
    body["username"] = serde_json::json!("ada");
    body["setupSecret"] = serde_json::json!(server.secret);

    let setup = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let response = setup
        .post(format!("{}/api/setup", server.url))
        .header("x-drop-request", "1")
        .json(&body)
        .send()
        .unwrap();
    assert!(response.status().is_success(), "setup {}", response.status());

    let mut client = DropClient::connect(&server.url).unwrap();
    assert!(format!("{client:?}").contains("unlocked: false"));
    let wrong = client.sign_in("ada", "not-the-password");
    assert!(wrong.is_err());
    assert!(!client.is_unlocked());

    let snapshot = client.sign_in("ada", password).unwrap();
    assert_eq!(snapshot.account.username, "ada");
    assert!(snapshot.http);
    let debug = format!("{client:?}");
    assert!(!debug.contains(password));
    assert!(!debug.contains(&bytes_to_b64url(material.content_key.as_slice())));

    let text = "meet at the north door";
    client.upload_text(text).unwrap();
    let listed = client.refresh().unwrap();
    assert_eq!(listed.items.len(), 1);
    let copied = match client.copy_item(&listed.items[0].id).unwrap() {
        drop_core::CopyPayload::Text(value) => value,
        _ => panic!("text item"),
    };
    assert_eq!(copied.as_str(), text);

    let browser_blob = browser_crypt(
        &repo_root(),
        &serde_json::json!({
            "op": "encrypt",
            "key": hex(material.content_key.as_slice()),
            "kind": "file",
            "name": "photo.bin",
            "mime": "application/octet-stream",
            "body": hex(&[1, 2, 3, 255, 0, 9]),
        }),
    );
    let browser_plain = decrypt(&material.content_key, &hex_decode(&browser_blob)).unwrap();
    let browser_item = decode_item(&browser_plain).unwrap();
    assert_eq!(browser_item.kind, ItemKind::File);
    assert_eq!(browser_item.body, vec![1, 2, 3, 255, 0, 9]);

    let native_blob = encrypt(
        &material.content_key,
        &encode_item(&ItemPlain {
            kind: ItemKind::Text,
            name: String::new(),
            mime: "text/plain".into(),
            body: text.as_bytes().to_vec(),
        })
        .unwrap(),
    )
    .unwrap();
    let browser_view = browser_crypt(
        &repo_root(),
        &serde_json::json!({
            "op": "decrypt",
            "key": hex(material.content_key.as_slice()),
            "blob": hex(&native_blob),
        }),
    );
    let viewed: serde_json::Value = serde_json::from_str(&browser_view).unwrap();
    assert_eq!(viewed["text"], text);
    assert_eq!(viewed["kind"], "text");

    client.sign_out();
    assert!(!client.is_unlocked());
    assert!(client.refresh().is_err());

    let stored = read_dir_bytes(&server.data_dir);
    assert!(!stored.windows(text.len()).any(|window| window == text.as_bytes()));
    assert!(!stored.windows(password.len()).any(|window| window == password.as_bytes()));
    let key_hex = hex(material.content_key.as_slice());
    assert!(!stored.windows(key_hex.len()).any(|window| window == key_hex.as_bytes()));
}

fn start_server() -> Server {
    let root = repo_root();
    assert!(
        root.join("node_modules/.bin/tsx").exists() || root.join("node_modules/tsx").exists(),
        "npm install is required before the server round-trip"
    );
    let data_dir = std::env::temp_dir().join(format!("drop-native-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    std::fs::create_dir_all(&data_dir).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let secret = format!("setup-secret-{}-{}", std::process::id(), std::process::id().wrapping_mul(97));
    let url = format!("http://127.0.0.1:{port}");
    let child = Command::new("npx")
        .arg("tsx")
        .arg("src/server/index.ts")
        .current_dir(&root)
        .env("SETUP_SECRET", &secret)
        .env("PORT", port.to_string())
        .env("HOST", "127.0.0.1")
        .env("COOKIE_SECURE", "false")
        .env("TRUST_PROXY", "false")
        .env("PUBLIC_URL", &url)
        .env("DATA_DIR", &data_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start drop server");
    let server = Server {
        child,
        url: url.clone(),
        data_dir,
        secret,
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if let Ok(response) = reqwest::blocking::get(format!("{url}/api/health")) {
            if response.status().is_success() {
                return server;
            }
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    panic!("server did not become healthy");
}

fn browser_crypt(root: &std::path::Path, payload: &serde_json::Value) -> String {
    let mut child = Command::new("npx")
        .arg("tsx")
        .arg("clients/core/tests/compat.ts")
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("tsx");
    child.stdin.take().unwrap().write_all(payload.to_string().as_bytes()).unwrap();
    let mut stdout = String::new();
    let mut stderr = String::new();
    child.stdout.take().unwrap().read_to_string(&mut stdout).unwrap();
    child.stderr.take().unwrap().read_to_string(&mut stderr).unwrap();
    let status = child.wait().unwrap();
    assert!(status.success(), "compat helper failed: {stderr}");
    stdout
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_dir_bytes(dir: &std::path::Path) -> Vec<u8> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(dir).unwrap();
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_file() {
            out.extend(std::fs::read(&path).unwrap());
        }
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hex_decode(value: &str) -> Vec<u8> {
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).unwrap())
        .collect()
}
