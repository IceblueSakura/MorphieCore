//! Optional CLI input transport; the existing callback owns OAuth validation.
use super::Error;
use crossterm::terminal;
use morphiecore::credential::SecretBytes;
use std::io::{self, BufRead, IsTerminal, Write};
use tokio::sync::oneshot;
use url::Url;

const INPUT_LIMIT: usize = 8192;

pub(super) async fn paste(receiver: oneshot::Receiver<String>) -> Result<(), Error> {
    let redirect = receiver
        .await
        .map_err(|_| Error::CallbackInput("browser callback is unavailable"))?;
    let expected =
        Url::parse(&redirect).map_err(|_| Error::CallbackInput("browser callback is invalid"))?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|_| Error::CallbackInput("local callback client is unavailable"))?;
    eprintln!(
        "For headless login, open the authorization URL on your computer. After signing in, \
         copy the full callback URL from the address bar even if the page cannot load. \
         The browser callback remains active."
    );
    loop {
        let Some(input) = read_input().await? else {
            eprintln!("Input closed; still waiting for the browser callback.");
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
        let response = match client.get(target).send().await {
            Ok(response) => response,
            Err(_) => {
                // A browser may already have claimed the callback and released the
                // listener while its code exchange is still pending. Only the
                // managed operation may decide that login's result.
                eprintln!(
                    "Local callback relay is unavailable; waiting for the browser login to finish. Ctrl-C cancels."
                );
                return Ok(());
            }
        };
        if response.status().is_success() {
            return Ok(());
        }
        eprintln!("Callback rejected; check that the URL belongs to this login and try again.");
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
    let terminal = io::stdin().is_terminal();
    let _guard = if terminal {
        terminal::enable_raw_mode()
            .map_err(|_| Error::CallbackInput("could not hide terminal input"))?;
        Some(TerminalGuard)
    } else {
        None
    };
    print_prompt()?;
    let (sender, receiver) = oneshot::channel();
    // Unlike tokio::io::stdin, this process-owned reader cannot hold runtime
    // shutdown open if the callback wins while a pipe remains open.
    std::thread::Builder::new()
        .name("oauth-callback-input".into())
        .spawn(move || {
            let result = if terminal {
                read_terminal_line(io::stdin().lock())
            } else {
                read_line(io::stdin().lock())
            };
            let _ = sender.send(result);
        })
        .map_err(|_| Error::CallbackInput("could not start input reader"))?;
    receiver
        .await
        .map_err(|_| Error::CallbackInput("input reader stopped"))?
}

fn print_prompt() -> Result<(), Error> {
    eprint!("Callback URL (input hidden): ");
    io::stderr()
        .flush()
        .map_err(|_| Error::CallbackInput("could not display input prompt"))
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

struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        eprintln!();
    }
}

fn read_terminal_line(mut reader: impl BufRead) -> Result<Option<SecretBytes>, Error> {
    // Read bytes, not whole paste events: an unfinished escape/paste sequence
    // must not accumulate an unbounded library buffer or swallow keyboard Ctrl-C.
    let mut input = SecretBytes::new(Vec::new());
    while let Some(byte) = reader
        .fill_buf()
        .map_err(|_| Error::CallbackInput("could not read terminal input"))?
        .first()
        .copied()
    {
        reader.consume(1);
        match byte {
            b'\r' | b'\n' => return Ok(Some(input)),
            3 => return Err(morphiecore::credential::CredentialError::Superseded.into()),
            4 if input.is_empty() => return Ok(None),
            21 => input.clear(),
            8 | 127 => {
                while let Some(byte) = input.pop() {
                    if byte & 0xc0 != 0x80 {
                        break;
                    }
                }
            }
            _ => {
                if input.len() == INPUT_LIMIT {
                    return Err(Error::CallbackInput("callback URL exceeds 8192 bytes"));
                }
                input.push(byte);
            }
        }
    }
    Ok(if input.is_empty() { None } else { Some(input) })
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
    fn input_is_bounded_even_with_an_unfinished_terminal_escape_sequence() {
        assert!(read_line(io::Cursor::new([])).unwrap().is_none());
        assert_eq!(
            &*read_line(io::Cursor::new(b"callback\r\n"))
                .unwrap()
                .unwrap(),
            b"callback"
        );
        let oversized = vec![b'a'; INPUT_LIMIT + 4];
        assert!(read_line(io::Cursor::new(&oversized)).is_err());
        let mut unfinished_paste = b"\x1b[200~".to_vec();
        unfinished_paste.extend_from_slice(&oversized);
        assert!(read_terminal_line(io::Cursor::new(unfinished_paste)).is_err());
        assert!(matches!(
            read_terminal_line(io::Cursor::new(b"\x1b[200~partial\x03")),
            Err(Error::Credential(
                morphiecore::credential::CredentialError::Superseded
            ))
        ));
    }
}
