//! Isolated proxy environment checks; never mutate the test runner's environment.
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

pub async fn matrix(child_test: &str) {
    for (variable, bypass, explicit, isolated) in [
        ("HTTP_PROXY", None, false, false),
        ("http_proxy", None, false, false),
        ("HTTPS_PROXY", None, false, false),
        ("https_proxy", None, false, false),
        ("ALL_PROXY", None, false, false),
        ("all_proxy", None, false, false),
        ("HTTP_PROXY", Some("NO_PROXY"), false, false),
        ("http_proxy", Some("no_proxy"), false, false),
        ("HTTP_PROXY", Some("NO_PROXY"), true, false),
        ("HTTP_PROXY", None, false, true),
    ] {
        let proxy = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_url = format!("http://{}", proxy.local_addr().unwrap());
        let tls = variable.eq_ignore_ascii_case("HTTPS_PROXY");
        let scheme = if tls { "https" } else { "http" };
        let origin = format!("{scheme}://{}", target.local_addr().unwrap());
        let direct = isolated || (bypass.is_some() && !explicit);
        let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", child_test, "--ignored", "--nocapture"])
            .env_clear()
            .env(variable, &proxy_url)
            .env("MORPHIECORE_TEST_PROXY_TARGET", format!("{origin}/fixture"))
            .env(
                "MORPHIECORE_TEST_PROXY_STATUS",
                if tls {
                    "0"
                } else if direct {
                    "204"
                } else {
                    "502"
                },
            )
            .kill_on_drop(true);
        if isolated {
            command.env("MORPHIECORE_TEST_PROXY_ISOLATED", "1");
        }
        if let Some(bypass) = bypass {
            command.env(bypass, "127.0.0.1");
        }
        if explicit {
            command.env("MORPHIECORE_TEST_PROXY_OVERRIDE", &proxy_url);
            command.env(variable, "http://127.0.0.1:1");
        }
        let serve = async {
            let chosen = if direct { &target } else { &proxy };
            let (mut stream, _) = chosen.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let mut chunk = [0; 1024];
                let size = stream.read(&mut chunk).await.unwrap();
                assert!(size > 0 && request.len() + size <= 8192);
                request.extend_from_slice(&chunk[..size]);
                if request.windows(4).any(|part| part == b"\r\n\r\n") {
                    break;
                }
            }
            let expected = if tls {
                format!("CONNECT {} HTTP/1.1\r\n", target.local_addr().unwrap())
            } else if direct {
                "GET /fixture HTTP/1.1\r\n".to_owned()
            } else {
                format!("GET {origin}/fixture HTTP/1.1\r\n")
            };
            assert!(request.starts_with(expected.as_bytes()));
            let response = if direct {
                b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".as_slice()
            } else {
                b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}"
                    .as_slice()
            };
            stream.write_all(response).await.unwrap();
        };
        let (output, ()) = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(command.output(), serve)
        })
        .await
        .expect("bounded isolated proxy scenario");
        let output = output.unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        // The other listener must never receive a fallback request.
        assert!(
            tokio::time::timeout(
                Duration::from_millis(20),
                if direct {
                    proxy.accept()
                } else {
                    target.accept()
                }
            )
            .await
            .is_err()
        );
    }
}

pub fn expected_status() -> u16 {
    std::env::var("MORPHIECORE_TEST_PROXY_STATUS")
        .unwrap()
        .parse()
        .unwrap()
}
