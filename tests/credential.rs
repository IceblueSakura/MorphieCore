//! Actual CLI and independent file expectations; all credentials are synthetic.
#[cfg(unix)]
#[path = "credential/api_keys.rs"]
mod api_keys;
#[cfg(unix)]
#[path = "../examples/support/child_process.rs"]
mod child_process;
#[cfg(unix)]
#[path = "support/filesystem.rs"]
mod test_files;
#[cfg(unix)]
mod unix {
    use serde_json::{Value, json};
    use std::{fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt, path::Path};
    use tokio::{
        process::Command,
        time::{Duration, timeout},
    };

    use crate::test_files::Directory;
    async fn command(args: &[&std::ffi::OsStr]) -> std::process::Output {
        command_with_home(args, None, None).await
    }
    async fn command_with_home(
        args: &[&std::ffi::OsStr],
        home: Option<&std::ffi::OsStr>,
        cwd: Option<&Path>,
    ) -> std::process::Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_morphiecore-auth"));
        command
            .args(args)
            .env_clear()
            .env("XAI_API_KEY", "unused-synthetic-key")
            .kill_on_drop(true);
        if let Some(home) = home {
            command.env("HOME", home);
        }
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }
        crate::child_process::run(&mut command, b"", Duration::from_secs(10), 64 << 10)
            .await
            .expect("bounded CLI operation")
    }
    fn save(root: &Path, profile: &str, alias: &str, value: &Value) {
        let path = root.join(format!("{profile}.json"));
        let mut document: Value = if path.exists() {
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap()
        } else {
            json!({"provider":profile,"revision":1,"oauth":{},"api_keys":{},"pools":{}})
        };
        document["oauth"][alias] = value.clone();
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .unwrap();
        file.write_all(&serde_json::to_vec_pretty(&document).unwrap())
            .unwrap();
    }
    fn account(profile: &str, alias: &str, subject: &str) -> Value {
        let siwc = profile == "openai";
        json!({
            "profile":profile,"alias":alias,
            "client_id":if siwc { "oaiapp_synthetic" } else { "synthetic-client" },
            "identity":{"subject":subject,"scope":null},
            "revision":2,"generation":1,"login_attempt":null,"state":"active",
            "credential":{"access":"synthetic-access","refresh":"synthetic-refresh",
                "id_token":if siwc {Some("synthetic-id-token")} else {None},
                "expires_at":4000000000u64,
                "scopes":if siwc {"openid profile email offline_access resource.invoke chatgpt.tokens.use.direct"} else {"openid profile email offline_access grok-cli:access api:access"}}
        })
    }
    fn read(root: &Path, profile: &str, alias: &str) -> Value {
        serde_json::from_slice::<Value>(
            &std::fs::read(root.join(format!("{profile}.json"))).unwrap(),
        )
        .unwrap()["oauth"][alias]
            .take()
    }
    #[tokio::test]
    async fn default_directory_reads_only_the_selected_home_and_explicit_store_wins() {
        use std::os::unix::fs::PermissionsExt;
        let home = Directory::existing();
        let cwd = Directory::existing();
        let root = home.path.join(".local/share/morphiecore/credentials");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        save(
            &root,
            "grok",
            "home",
            &account("grok", "home", "synthetic-home-person"),
        );
        save(
            &cwd.path,
            "grok",
            "override",
            &account("grok", "override", "synthetic-override-person"),
        );
        let args = ["list".as_ref()];
        let output = command_with_home(&args, Some(home.path.as_os_str()), Some(&cwd.path)).await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let statuses: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(statuses.as_array().unwrap().len(), 1);
        assert_eq!(statuses[0]["account"], "home");
        assert_eq!(statuses[0]["access"], "valid");
        assert!(!String::from_utf8_lossy(&output.stdout).contains("synthetic-"));

        let args = ["list".as_ref(), "--store".as_ref(), cwd.path.as_os_str()];
        for selected_home in [Some(home.path.as_os_str()), None] {
            let output = command_with_home(&args, selected_home, Some(&cwd.path)).await;
            assert!(output.status.success());
            let statuses: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(statuses.as_array().unwrap().len(), 1);
            assert_eq!(statuses[0]["account"], "override");
        }
    }
    #[tokio::test]
    async fn default_directory_rejects_missing_invalid_home_and_unsafe_store_without_fallback() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let cwd = Directory::existing();
        save(
            &cwd.path,
            "grok",
            "decoy",
            &account("grok", "decoy", "synthetic-decoy"),
        );
        for home in [None, Some("".as_ref()), Some("relative-home".as_ref())] {
            let output = command_with_home(&["list".as_ref()], home, Some(&cwd.path)).await;
            assert!(!output.status.success());
            assert!(output.stdout.is_empty());
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("default credential directory")
            );
        }
        let home = Directory::existing();
        let root = home.path.join(".local/share/morphiecore/credentials");
        std::fs::create_dir_all(root.parent().unwrap()).unwrap();
        symlink(&cwd.path, &root).unwrap();
        let output = command_with_home(
            &["list".as_ref()],
            Some(home.path.as_os_str()),
            Some(&cwd.path),
        )
        .await;
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        std::fs::remove_file(&root).unwrap();
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        save(
            &root,
            "grok",
            "private",
            &account("grok", "private", "synthetic-private"),
        );
        std::fs::set_permissions(
            root.join("grok.json"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        let output = command_with_home(
            &["list".as_ref()],
            Some(home.path.as_os_str()),
            Some(&cwd.path),
        )
        .await;
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("synthetic-"));
    }
    #[tokio::test]
    async fn binary_uses_account_files_and_account_locks_without_cross_profile_effects() {
        let dir = Directory::existing();
        save(
            &dir.path,
            "grok",
            "one",
            &account("grok", "one", "synthetic-person-one"),
        );
        save(
            &dir.path,
            "grok",
            "two",
            &account("grok", "two", "synthetic-person-two"),
        );
        save(
            &dir.path,
            "openai",
            "one",
            &account("openai", "one", "synthetic-person-one"),
        );
        let path = dir.path.as_os_str();
        let all = command(&["list".as_ref(), "--store".as_ref(), path]).await;
        assert!(all.status.success());
        let statuses: Value = serde_json::from_slice(&all.stdout).unwrap();
        assert_eq!(statuses.as_array().unwrap().len(), 3);
        for secret in [
            "synthetic-person",
            "synthetic-access",
            "synthetic-refresh",
            "synthetic-id-token",
            "synthetic-workspace",
            "unused-synthetic-key",
        ] {
            assert!(!String::from_utf8_lossy(&all.stdout).contains(secret));
            assert!(!String::from_utf8_lossy(&all.stderr).contains(secret));
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(dir.path.join("grok.oauth.one.lock"))
            .unwrap();
        lock.lock().unwrap();
        let busy = command(&[
            "grok".as_ref(),
            "logout".as_ref(),
            "--store".as_ref(),
            path,
            "--account".as_ref(),
            "one".as_ref(),
        ])
        .await;
        assert!(!busy.status.success());
        assert!(String::from_utf8_lossy(&busy.stderr).contains("store is busy"));
        let list = command(&["grok".as_ref(), "list".as_ref(), "--store".as_ref(), path]).await;
        assert!(list.status.success());
        let unrelated = command(&[
            "grok".as_ref(),
            "logout".as_ref(),
            "--store".as_ref(),
            path,
            "--account".as_ref(),
            "two".as_ref(),
        ])
        .await;
        assert!(unrelated.status.success());
        assert!(read(&dir.path, "grok", "two")["credential"].is_null());
        assert_eq!(
            read(&dir.path, "grok", "one")["credential"]["refresh"],
            "synthetic-refresh"
        );
        assert_eq!(
            read(&dir.path, "openai", "one")["credential"]["refresh"],
            "synthetic-refresh"
        );
        drop(lock);
        let logout = command(&[
            "grok".as_ref(),
            "logout".as_ref(),
            "--store".as_ref(),
            path,
            "--account".as_ref(),
            "one".as_ref(),
        ])
        .await;
        assert!(logout.status.success());
        let snapshot = read(&dir.path, "grok", "one");
        assert_eq!(snapshot["state"], "signed_out");
        assert!(snapshot["credential"].is_null());
        assert_eq!(snapshot["identity"]["subject"], "synthetic-person-one");
        let refresh = command(&[
            "grok".as_ref(),
            "refresh".as_ref(),
            "--store".as_ref(),
            path,
            "--account".as_ref(),
            "one".as_ref(),
        ])
        .await;
        assert!(!refresh.status.success());
        assert!(String::from_utf8_lossy(&refresh.stderr).contains("login required"));
        let store_lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(dir.path.join("store.lock"))
            .unwrap();
        store_lock.lock().unwrap();
        let busy = command(&["list".as_ref(), "--store".as_ref(), path]).await;
        assert!(!busy.status.success());
    }
    #[tokio::test]
    async fn binary_refuses_old_format_and_reports_quarantine_without_exposing_tokens() {
        let legacy = Directory::existing();
        std::fs::write(legacy.path.join("accounts.json"), "synthetic obsolete data").unwrap();
        let output = command(&["list".as_ref(), "--store".as_ref(), legacy.path.as_os_str()]).await;
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("legacy"));
        assert_eq!(
            std::fs::read_to_string(legacy.path.join("accounts.json")).unwrap(),
            "synthetic obsolete data"
        );

        let dir = Directory::existing();
        save(
            &dir.path,
            "openai",
            "one",
            &account("openai", "one", "synthetic-person"),
        );
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(dir.path.join("openai.oauth.one.pending"))
            .unwrap()
            .write_all(b"true")
            .unwrap();
        let output = command(&["list".as_ref(), "--store".as_ref(), dir.path.as_os_str()]).await;
        assert!(output.status.success());
        let statuses: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(statuses[0]["state"], "needs_reauthorization");
        assert_eq!(statuses[0]["access"], "unavailable");
        assert_eq!(statuses[0]["recovery_required"], true);
        let logout = command(&[
            "openai".as_ref(),
            "logout".as_ref(),
            "--store".as_ref(),
            dir.path.as_os_str(),
            "--account".as_ref(),
            "one".as_ref(),
            "--revoke".as_ref(),
        ])
        .await;
        assert!(!logout.status.success());
        assert!(String::from_utf8_lossy(&logout.stdout).contains("NOT confirmed"));
        assert!(read(&dir.path, "openai", "one")["credential"].is_null());
        assert_eq!(
            std::fs::read(dir.path.join("openai.oauth.one.pending")).unwrap(),
            b"false"
        );
    }

    #[tokio::test]
    async fn browser_registration_errors_are_rejected_before_store_creation() {
        let dir = Directory::existing();
        let store = dir.path.join("not-created");
        for flags in [
            vec![
                "codex",
                "login",
                "--method",
                "browser",
                "--callback-port",
                "0",
            ],
            vec!["siwc", "login", "--method", "browser"],
            vec!["openai", "login", "--method", "device"],
            vec![
                "openai",
                "login",
                "--method",
                "browser",
                "--client-id",
                "unrelated-client",
            ],
            vec![
                "grok",
                "login",
                "--method",
                "browser",
                "--client-id",
                "invalid client",
            ],
        ] {
            let mut args: Vec<&std::ffi::OsStr> = flags.iter().map(AsRef::as_ref).collect();
            args.extend([
                "--store".as_ref(),
                store.as_os_str(),
                "--account".as_ref(),
                "personal".as_ref(),
            ]);
            let result = command(&args).await;
            assert!(!result.status.success());
            assert!(result.stdout.is_empty());
            assert!(!store.exists());
        }
    }

    #[tokio::test]
    async fn siwc_cli_defaults_to_browser_and_reuses_host_without_authority_traffic() {
        use std::process::Stdio;
        use tokio::{
            io::{AsyncBufReadExt, AsyncReadExt, BufReader},
            net::TcpListener,
        };
        let dir = Directory::existing();
        let store = dir.path.join("sessions");
        let egress = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy = format!("http://{}", egress.local_addr().unwrap());
        let mut original_host = None;
        for consent in [false, true] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_morphiecore-auth"));
            command
                .args(["openai", "login", "--account", "personal", "--store"])
                .arg(&store)
                .args(["--proxy", &proxy]);
            if consent {
                command.arg("--consent");
            }
            let mut child = command
                .env_clear()
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .unwrap();
            let mut stderr = BufReader::new(child.stderr.take().unwrap().take(8192));
            let authorization = timeout(Duration::from_secs(5), async {
                for _ in 0..10 {
                    let mut line = String::new();
                    assert!(stderr.read_line(&mut line).await.unwrap() > 0);
                    if line.starts_with("https://auth.openai.com/") {
                        return url::Url::parse(line.trim()).unwrap();
                    }
                }
                panic!("missing SIWC authorization prompt");
            })
            .await
            .unwrap();
            assert_eq!(authorization.path(), "/api/accounts/authorize");
            let fields: std::collections::BTreeMap<_, _> =
                authorization.query_pairs().into_owned().collect();
            assert_eq!(fields["client_id"], "dynamic_agent_client");
            assert_eq!(fields["agent_name_hint"], "MorphieCore");
            assert_eq!(
                fields.get("prompt").map(String::as_str),
                consent.then_some("consent")
            );
            let host = fields["ext_agent_host_id"].clone();
            if let Some(previous) = &original_host {
                assert_eq!(previous, &host);
            }
            original_host = Some(host);
            let uri = url::Url::parse(&fields["redirect_uri"]).unwrap();
            assert_eq!(uri.host_str(), Some("127.0.0.1"));
            assert_eq!(uri.path(), "/auth/callback");
            let address = std::net::SocketAddr::from(([127, 0, 0, 1], uri.port().unwrap()));
            let response = reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap()
                .get(format!(
                    "{}?state=unrelated&error=access_denied",
                    fields["redirect_uri"]
                ))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 400);
            assert_eq!(
                unsafe { libc::kill(child.id().unwrap() as libc::pid_t, libc::SIGINT) },
                0
            );
            assert!(
                !timeout(Duration::from_secs(5), child.wait())
                    .await
                    .unwrap()
                    .unwrap()
                    .success()
            );
            let account = read(&store, "openai", "personal");
            assert!(account["login_attempt"].is_null());
            assert!(account["credential"].is_null());
            drop(TcpListener::bind(address).await.unwrap());
        }
        assert!(
            timeout(Duration::from_millis(20), egress.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn binary_browser_callback_and_ctrl_c_are_owned_without_authority_traffic() {
        use std::process::Stdio;
        use tokio::{
            io::{AsyncBufReadExt, AsyncReadExt, BufReader},
            net::TcpListener,
        };
        let dir = Directory::existing();
        let store = dir.path.join("sessions");
        let egress = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy = format!("http://{}", egress.local_addr().unwrap());
        let mut child = Command::new(env!("CARGO_BIN_EXE_morphiecore-auth"))
            .args([
                "grok",
                "login",
                "--account",
                "personal",
                "--method",
                "browser",
                "--callback-port",
                "0",
                "--store",
            ])
            .arg(&store)
            .args(["--proxy", &proxy])
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut stderr = BufReader::new(child.stderr.take().unwrap().take(8192));
        let mut authorization = None;
        let redirect = timeout(Duration::from_secs(5), async {
            for _ in 0..4 {
                let mut line = String::new();
                assert!(stderr.read_line(&mut line).await.unwrap() > 0);
                if line.starts_with("https://auth.x.ai/") {
                    authorization = Some(url::Url::parse(line.trim()).unwrap());
                }
                if let Some(uri) = line.strip_prefix("Callback: ") {
                    return uri.trim().to_owned();
                }
            }
            panic!("browser callback prompt missing");
        })
        .await
        .unwrap();
        let authorization = authorization.unwrap();
        let fields: std::collections::BTreeMap<_, _> =
            authorization.query_pairs().into_owned().collect();
        assert_eq!(fields["client_id"], "b1a00492-073a-47ea-816f-4c329264a828");
        assert_eq!(fields["referrer"], "grok-build");
        assert_eq!(
            fields["scope"],
            "openid profile email offline_access grok-cli:access api:access"
        );
        let uri = url::Url::parse(&redirect).unwrap();
        assert_eq!(uri.scheme(), "http");
        assert_eq!(uri.host_str(), Some("127.0.0.1"));
        assert_eq!(uri.path(), "/callback");
        let address = std::net::SocketAddr::from(([127, 0, 0, 1], uri.port().unwrap()));
        let response = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap()
            .get(format!("{redirect}?state=unrelated&error=access_denied"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
        assert!(child.try_wait().unwrap().is_none());
        // This PID belongs to the child above; kill_on_drop is the failure-path guard.
        assert_eq!(
            unsafe { libc::kill(child.id().unwrap() as libc::pid_t, libc::SIGINT) },
            0
        );
        assert!(
            !timeout(Duration::from_secs(5), child.wait())
                .await
                .unwrap()
                .unwrap()
                .success()
        );
        let account = read(&store, "grok", "personal");
        assert!(account["login_attempt"].is_null());
        assert!(account["credential"].is_null());
        assert_eq!(account["state"], "signed_out");
        drop(TcpListener::bind(address).await.unwrap());
        assert!(
            timeout(Duration::from_millis(20), egress.accept())
                .await
                .is_err()
        );
    }
}
