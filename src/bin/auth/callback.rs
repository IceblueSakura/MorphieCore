//! Browser login input; stdin and HTTP share the existing callback validation.
use super::Error;
use morphiecore::credential::SecretBytes;
use std::io::{self, BufRead};
use tokio::sync::{mpsc, oneshot};
use url::Url;

const INPUT_LIMIT: usize = 8192;

pub(super) async fn input(mut receiver: mpsc::Receiver<String>) -> Result<(), Error> {
    let redirect = receiver
        .recv()
        .await
        .ok_or(Error::CallbackInput("browser callback is unavailable"))?;
    let expected =
        Url::parse(&redirect).map_err(|_| Error::CallbackInput("browser callback is invalid"))?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|_| Error::CallbackInput("local callback client is unavailable"))?;
    loop {
        eprintln!("Callback URL (or finish in the browser):");
        let Some(input) = read_input().await? else {
            return Ok(());
        };
        let Some(target) = callback_url(&input, &expected) else {
            eprintln!(
                "Paste the full URL for this callback, including its query; do not paste a key or token."
            );
            continue;
        };
        // Always use the prepared loopback target, never an input-selected origin.
        // State, duplicate fields and issued client binding keep their existing owner.
        match client.get(target).send().await {
            Ok(response) if !response.status().is_success() => {
                eprintln!(
                    "Callback rejected; check that the URL belongs to this login and try again."
                );
            }
            // The shared callback owns the first accepted result and final login outcome.
            _ => return Ok(()),
        }
    }
}

fn callback_url(input: &[u8], expected: &Url) -> Option<Url> {
    let input = std::str::from_utf8(input).ok()?.trim();
    if input.is_empty() || input.len() > INPUT_LIMIT || input.bytes().any(|c| c.is_ascii_control())
    {
        return None;
    }
    let parsed = Url::parse(input).ok()?;
    if expected.scheme() != "http"
        || expected.host_str() != Some("127.0.0.1")
        || parsed.origin() != expected.origin()
        || parsed.path() != expected.path()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
        || parsed.query().is_none()
    {
        return None;
    }
    let mut target = expected.clone();
    target.set_query(parsed.query());
    Some(target)
}

async fn read_input() -> Result<Option<SecretBytes>, Error> {
    let (sender, receiver) = oneshot::channel();
    // A plain thread keeps a pending stdin read outside Tokio's shutdown-owned pool.
    std::thread::Builder::new()
        .name("oauth-callback-input".into())
        .spawn(move || {
            let _ = sender.send(read_line(io::stdin().lock()));
        })
        .map_err(|_| Error::CallbackInput("could not start input reader"))?;
    receiver
        .await
        .map_err(|_| Error::CallbackInput("input reader stopped"))?
}

fn read_line(reader: impl BufRead) -> Result<Option<SecretBytes>, Error> {
    let mut bytes = SecretBytes::new(Vec::new());
    reader
        .take((INPUT_LIMIT + 3) as u64)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| Error::CallbackInput("could not read callback URL"))?;
    if bytes.is_empty() {
        return Ok(None);
    }
    if bytes.ends_with(b"\n") {
        bytes.pop();
    }
    if bytes.ends_with(b"\r") {
        bytes.pop();
    }
    if bytes.len() > INPUT_LIMIT {
        return Err(Error::CallbackInput("callback URL exceeds 8192 bytes"));
    }
    Ok(Some(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pasted_input_cannot_choose_a_target_or_bypass_callback_validation() {
        let expected = Url::parse("http://127.0.0.1:1455/auth/callback").unwrap();
        for input in [
            "synthetic-code",
            "code=synthetic-code&state=expected",
            "http://127.0.0.1:1456/auth/callback?code=one&state=expected",
            "http://localhost:1455/auth/callback?code=one&state=expected",
            "https://127.0.0.1:1455/auth/callback?code=one&state=expected",
            "http://127.0.0.1:1455/other?code=one&state=expected",
            "http://private@127.0.0.1:1455/auth/callback?code=one&state=expected",
            "http://127.0.0.1:1455/auth/callback?code=one&state=expected#fragment",
            "http://127.0.0.1:1455/auth/callback?code=one\n&state=expected",
        ] {
            assert!(callback_url(input.as_bytes(), &expected).is_none());
        }
        let input =
            b"http://127.0.0.1:1455/auth/callback?state=wrong&code=one&code=two&client_id=issued";
        let target = callback_url(input, &expected).unwrap();
        assert_eq!(
            target.query(),
            Some("state=wrong&code=one&code=two&client_id=issued")
        );
    }

    #[test]
    fn input_lines_are_bounded_and_accept_crlf() {
        assert!(read_line(io::Cursor::new([])).unwrap().is_none());
        assert_eq!(
            &*read_line(io::Cursor::new(b"callback\r\n"))
                .unwrap()
                .unwrap(),
            b"callback"
        );
        let oversized = vec![b'a'; INPUT_LIMIT + 4];
        assert!(read_line(io::Cursor::new(&oversized)).is_err());
    }
}
