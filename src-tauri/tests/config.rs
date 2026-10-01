use std::path::{Path, PathBuf};

use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("raiz do repositorio")
        .to_path_buf()
}

fn json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("leitura")).expect("json valido")
}

fn tauri_conf() -> Value {
    json(&root().join("src-tauri").join("tauri.conf.json"))
}

#[test]
fn csp_allows_only_self_ipc_and_soundcloud_artwork() {
    let conf = tauri_conf();
    let csp = &conf["app"]["security"]["csp"];
    assert_eq!(csp["default-src"], "'self'");
    assert_eq!(
        csp["script-src"], "'self'",
        "sem unsafe-inline/eval nem origem externa"
    );
    assert_eq!(csp["object-src"], "'none'");
    assert_eq!(csp["base-uri"], "'none'");
    assert_eq!(csp["form-action"], "'none'");
    assert_eq!(csp["img-src"], "'self' https://*.sndcdn.com");
    let connect = csp["connect-src"].as_str().expect("connect-src");
    assert!(
        connect
            .split_whitespace()
            .all(|source| source == "ipc:" || source == "http://ipc.localhost"),
        "o front nao fala com a rede, so com o IPC: {connect}"
    );
    let all = csp.to_string();
    assert!(!all.contains("unsafe"), "nenhuma diretiva unsafe-*: {all}");
    assert!(
        conf["app"]["security"]["freezePrototype"]
            .as_bool()
            .unwrap_or(false)
    );
    assert_eq!(
        conf["app"]["withGlobalTauri"], false,
        "API do Tauri nao exposta em window.__TAURI__"
    );
}

#[test]
fn ipc_capabilities_grant_only_event_listening() {
    let dir = root().join("src-tauri").join("capabilities");
    for entry in std::fs::read_dir(&dir).expect("capabilities") {
        let path = entry.expect("entrada").path();
        let capability = json(&path);
        let permissions: Vec<&str> = capability["permissions"]
            .as_array()
            .expect("permissions")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(
            permissions,
            ["core:event:allow-listen", "core:event:allow-unlisten"],
            "{} concede mais que ouvir eventos",
            path.display()
        );
        assert_eq!(capability["windows"], serde_json::json!(["main"]));
    }
}

#[test]
fn installer_is_per_user_without_admin_rights() {
    let conf = tauri_conf();
    assert_eq!(
        conf["bundle"]["windows"]["nsis"]["installMode"],
        "currentUser"
    );
    assert_eq!(conf["bundle"]["targets"], serde_json::json!(["nsis"]));
}

#[test]
fn versions_match_across_manifests() {
    let workspace = std::fs::read_to_string(root().join("Cargo.toml")).expect("Cargo.toml");
    let cargo_version = workspace
        .lines()
        .find_map(|line| line.strip_prefix("version = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("versao do workspace");
    assert_eq!(cargo_version, env!("CARGO_PKG_VERSION"));
    for manifest in [
        root().join("package.json"),
        root().join("web").join("package.json"),
    ] {
        assert_eq!(
            json(&manifest)["version"],
            cargo_version,
            "{} fora de sincronia",
            manifest.display()
        );
    }
    assert_eq!(
        tauri_conf()["version"],
        cargo_version,
        "tauri.conf.json fora de sincronia"
    );
}
