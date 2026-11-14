//! The node's one HTTPS client: a `GET` at a time over HTTP/1.1, for `--import`
//! to fetch Rootstock's signed state copies.
//!
//! The server's certificate is checked against Mozilla's roots (`webpki-roots`)
//! through rustls on `ring`. Rootstock serves its copies from S3 through
//! CloudFront, as plain `200` answers carrying a `Content-Length`; this also
//! reads chunked bodies and bodies that run to the close, and follows up to
//! five redirects, to `https://` only. Nothing it fetches is trusted for
//! arriving this way: `--import` holds every byte to what the signers signed.

use anyhow::{anyhow, bail, ensure, Context, Result};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tokio::net::TcpStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::{self, ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;

/// How long connecting, and then the TLS handshake, may each take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// How long the server may go quiet before the transfer is given up.
const READ_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_REDIRECTS: usize = 5;
/// The most the status line and the headers may take, together.
const MAX_HEAD: u64 = 64 * 1024;
/// The most a chunk-size line, or the trailers after the last chunk, may take.
const MAX_CHUNK_LINE: u64 = 4 * 1024;

/// Where a body goes as it arrives.
pub(crate) trait Sink: Send {
    fn write(&mut self, data: &[u8]) -> Result<()>;
}

/// An `https://` URL, split as a request needs it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Url {
    pub host: String,
    pub port: u16,
    /// The path and query, from the first `/`; `/` for a URL without one.
    pub path: String,
}

pub(crate) fn parse_url(url: &str) -> Result<Url> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| anyhow!("{url}: only https:// URLs are fetched"))?;
    let rest = rest.split('#').next().unwrap_or_default();
    let (authority, path) = match rest.find(['/', '?']) {
        Some(i) if rest.as_bytes()[i] == b'/' => (&rest[..i], rest[i..].to_string()),
        Some(i) => (&rest[..i], format!("/{}", &rest[i..])),
        None => (rest, "/".to_string()),
    };
    // Anything else in a request line would let the URL write headers of its own.
    ensure!(
        path.bytes().all(|b| b.is_ascii_graphic()),
        "{url}: the path has to be printable ASCII, without spaces"
    );
    ensure!(
        !authority.contains('@'),
        "{url}: a URL carrying credentials is not fetched"
    );
    ensure!(
        !authority.starts_with('['),
        "{url}: an IPv6 literal is not supported"
    );
    let (host, port) = match authority.split_once(':') {
        Some((host, port)) => (
            host,
            port.parse::<u16>()
                .with_context(|| format!("{url}: the port {port:?}"))?,
        ),
        None => (authority, 443),
    };
    ensure!(
        !host.is_empty()
            && host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.'),
        "{url}: the host has to be a name or an IPv4 address"
    );
    Ok(Url {
        host: host.to_ascii_lowercase(),
        port,
        path,
    })
}

/// What one request came to.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// The body went to the sink: this many bytes of it.
    Body(u64),
    /// The server pointed elsewhere.
    Redirect(String),
}

/// Fetch `url` into `sink`, following redirects. Returns the body's length.
pub(crate) async fn get(url: &str, sink: &mut dyn Sink) -> Result<u64> {
    let config = tls_config()?;
    let mut url = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        let target = parse_url(&url)?;
        let stream = connect(&target, &config).await?;
        match exchange(stream, &target, sink)
            .await
            .with_context(|| format!("fetching {url}"))?
        {
            Outcome::Body(n) => return Ok(n),
            Outcome::Redirect(location) => url = resolve(&target, &location)?,
        }
    }
    bail!("{url}: more than {MAX_REDIRECTS} redirects")
}

fn tls_config() -> Result<Arc<ClientConfig>> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .context("TLS: the default protocol versions")?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(config))
}

async fn connect(
    url: &Url,
    config: &Arc<ClientConfig>,
) -> Result<tokio_rustls::client::TlsStream<TcpStream>> {
    let (host, port) = (url.host.as_str(), url.port);
    let tcp = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| anyhow!("connecting to {host}:{port} timed out"))?
        .with_context(|| format!("connecting to {host}:{port}"))?;
    let name = ServerName::try_from(url.host.clone())
        .map_err(|_| anyhow!("{host} is not a name a certificate can be checked against"))?;
    tokio::time::timeout(
        CONNECT_TIMEOUT,
        TlsConnector::from(config.clone()).connect(name, tcp),
    )
    .await
    .map_err(|_| anyhow!("the TLS handshake with {host} timed out"))?
    .with_context(|| format!("TLS with {host}"))
}

async fn exchange<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    url: &Url,
    sink: &mut dyn Sink,
) -> Result<Outcome> {
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: rustock/{}\r\nAccept: */*\r\n\
         Accept-Encoding: identity\r\nConnection: close\r\n\r\n",
        url.path,
        authority(url),
        env!("CARGO_PKG_VERSION")
    );
    timed(stream.write_all(request.as_bytes())).await?;
    timed(stream.flush()).await?;
    let mut reader = BufReader::with_capacity(1 << 18, stream);
    read_response(&mut reader, sink).await
}

/// The host, and the port where it isn't 443, as a `Host` header names them.
fn authority(url: &Url) -> String {
    if url.port == 443 {
        url.host.clone()
    } else {
        format!("{}:{}", url.host, url.port)
    }
}

async fn timed<T>(f: impl Future<Output = std::io::Result<T>>) -> Result<T> {
    match tokio::time::timeout(READ_TIMEOUT, f).await {
        Ok(done) => Ok(done?),
        Err(_) => bail!("the server sent nothing for {} s", READ_TIMEOUT.as_secs()),
    }
}

async fn read_response<R: AsyncBufRead + Unpin>(r: &mut R, sink: &mut dyn Sink) -> Result<Outcome> {
    let mut budget = MAX_HEAD;
    let status_line = read_line(r, &mut budget).await?;
    let mut parts = status_line.splitn(3, ' ');
    let version = parts.next().unwrap_or_default();
    ensure!(
        version.starts_with("HTTP/1."),
        "not an HTTP/1 answer: {status_line:?}"
    );
    let status: u16 = parts
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow!("no status code in {status_line:?}"))?;

    let mut length: Option<u64> = None;
    let mut chunked = false;
    let mut location: Option<String> = None;
    loop {
        let line = read_line(r, &mut budget).await?;
        if line.is_empty() {
            break;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| anyhow!("a header line without a colon: {line:?}"))?;
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => {
                let n: u64 = value
                    .parse()
                    .with_context(|| format!("Content-Length {value:?}"))?;
                ensure!(
                    length.is_none_or(|l| l == n),
                    "two different Content-Length headers"
                );
                length = Some(n);
            }
            "transfer-encoding" => {
                let codings: Vec<String> = value
                    .split(',')
                    .map(|c| c.trim().to_ascii_lowercase())
                    .collect();
                ensure!(
                    codings == ["chunked"],
                    "Transfer-Encoding {value:?}: only chunked is read"
                );
                chunked = true;
            }
            "location" => location = Some(value.to_string()),
            _ => {}
        }
    }

    if matches!(status, 301 | 302 | 303 | 307 | 308) {
        return location
            .map(Outcome::Redirect)
            .ok_or_else(|| anyhow!("HTTP {status} without a Location"));
    }
    ensure!(status == 200, "HTTP {status}");
    // A chunked body says where it ends whatever the length says (RFC 9112).
    let n = if chunked {
        read_chunked(r, sink).await?
    } else if let Some(len) = length {
        read_exact_body(r, len, sink).await?
    } else {
        read_to_close(r, sink).await?
    };
    Ok(Outcome::Body(n))
}

/// One line, without its line ending, its bytes taken from `budget`.
async fn read_line<R: AsyncBufRead + Unpin>(r: &mut R, budget: &mut u64) -> Result<String> {
    let mut buf = Vec::new();
    let n = timed((&mut *r).take(*budget).read_until(b'\n', &mut buf)).await?;
    ensure!(n > 0, "the connection closed in the middle of an answer");
    ensure!(
        buf.ends_with(b"\n"),
        "the answer ends, or runs past its limit, inside a line"
    );
    *budget -= n as u64;
    while buf.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
        buf.pop();
    }
    String::from_utf8(buf).context("a line of the answer that isn't UTF-8")
}

async fn read_exact_body<R: AsyncBufRead + Unpin>(
    r: &mut R,
    len: u64,
    sink: &mut dyn Sink,
) -> Result<u64> {
    let mut left = len;
    while left > 0 {
        let buf = timed(r.fill_buf()).await?;
        ensure!(
            !buf.is_empty(),
            "the body ends after {} of {len} bytes",
            len - left
        );
        let n = buf.len().min(usize::try_from(left).unwrap_or(usize::MAX));
        sink.write(&buf[..n])?;
        r.consume(n);
        left -= n as u64;
    }
    Ok(len)
}

async fn read_chunked<R: AsyncBufRead + Unpin>(r: &mut R, sink: &mut dyn Sink) -> Result<u64> {
    let mut total = 0u64;
    loop {
        let mut budget = MAX_CHUNK_LINE;
        let line = read_line(r, &mut budget).await?;
        let size = line.split(';').next().unwrap_or_default().trim();
        ensure!(
            !size.is_empty() && size.bytes().all(|b| b.is_ascii_hexdigit()),
            "a chunk size {line:?}"
        );
        let size =
            u64::from_str_radix(size, 16).with_context(|| format!("a chunk size {line:?}"))?;
        if size == 0 {
            // The trailers, if any, then the blank line that ends the body.
            let mut budget = MAX_CHUNK_LINE;
            while !read_line(r, &mut budget).await?.is_empty() {}
            return Ok(total);
        }
        read_exact_body(r, size, sink).await?;
        total = total
            .checked_add(size)
            .context("a body of more than 2^64 bytes")?;
        let mut budget = 2;
        ensure!(
            read_line(r, &mut budget).await?.is_empty(),
            "a chunk runs past its size"
        );
    }
}

async fn read_to_close<R: AsyncBufRead + Unpin>(r: &mut R, sink: &mut dyn Sink) -> Result<u64> {
    let mut total = 0u64;
    loop {
        let buf = match tokio::time::timeout(READ_TIMEOUT, r.fill_buf()).await {
            Err(_) => bail!("the server sent nothing for {} s", READ_TIMEOUT.as_secs()),
            Ok(Ok(buf)) => buf,
            // rustls reports a close without TLS's close_notify as an
            // unexpected end; for a body that runs to the close, it is the end.
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(total),
            Ok(Err(e)) => return Err(e.into()),
        };
        if buf.is_empty() {
            return Ok(total);
        }
        let n = buf.len();
        sink.write(buf)?;
        r.consume(n);
        total += n as u64;
    }
}

/// Where a redirect's `Location` points, from the URL that answered with it.
fn resolve(from: &Url, location: &str) -> Result<String> {
    if location.starts_with("https://") {
        Ok(location.to_string())
    } else if let Some(rest) = location.strip_prefix("//") {
        Ok(format!("https://{rest}"))
    } else if location.starts_with('/') {
        Ok(format!("https://{}{location}", authority(from)))
    } else {
        bail!("a redirect to {location:?}: only https:// URLs and absolute paths are followed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Sink for Vec<u8> {
        fn write(&mut self, data: &[u8]) -> Result<()> {
            self.extend_from_slice(data);
            Ok(())
        }
    }

    async fn answer(raw: &[u8]) -> (Result<Outcome>, Vec<u8>) {
        let mut body = Vec::new();
        let mut reader = BufReader::new(raw);
        let outcome = read_response(&mut reader, &mut body).await;
        (outcome, body)
    }

    #[test]
    fn urls_are_split_as_a_request_needs_them() {
        let url =
            parse_url("https://Import.Mainnet.rskcomputing.net/dbs/mainnet/k/index.json").unwrap();
        assert_eq!(
            url,
            Url {
                host: "import.mainnet.rskcomputing.net".into(),
                port: 443,
                path: "/dbs/mainnet/k/index.json".into(),
            }
        );
        let url = parse_url("https://example.org:8443?x=1#top").unwrap();
        assert_eq!((url.port, url.path.as_str()), (8443, "/?x=1"));
        assert_eq!(parse_url("https://example.org").unwrap().path, "/");
        for bad in [
            "http://example.org/",
            "https:///path",
            "https://user@example.org/",
            "https://[::1]/",
            "https://example.org:x/",
            "https://example.org/a b",
            "https://example.org/a\r\nX-Injected: 1",
            "https://exa mple.org/",
            "https://example.org\r\nX: 1/",
        ] {
            assert!(parse_url(bad).is_err(), "{bad} was taken");
        }
    }

    #[tokio::test]
    async fn a_body_of_a_given_length_is_read_whole_and_no_further() {
        let (outcome, body) =
            answer(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nServer: x\r\n\r\nhelloEXTRA").await;
        assert_eq!(outcome.unwrap(), Outcome::Body(5));
        assert_eq!(body, b"hello");
    }

    #[tokio::test]
    async fn a_body_shorter_than_its_length_is_refused() {
        let (outcome, _) = answer(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nhello").await;
        assert!(outcome.unwrap_err().to_string().contains("5 of 9"));
    }

    #[tokio::test]
    async fn a_chunked_body_is_put_back_together() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 1\r\n\r\n\
                    5;ext=1\r\nhello\r\n6\r\n world\r\n0\r\nTrailer: t\r\n\r\n";
        let (outcome, body) = answer(raw).await;
        assert_eq!(outcome.unwrap(), Outcome::Body(11));
        assert_eq!(body, b"hello world");
    }

    #[tokio::test]
    async fn a_chunk_that_overruns_its_size_or_a_bad_size_is_refused() {
        let overrun =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nhello\r\n0\r\n\r\n";
        assert!(answer(overrun).await.0.is_err());
        let signed = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n+3\r\nhey\r\n0\r\n\r\n";
        assert!(answer(signed).await.0.is_err());
    }

    #[tokio::test]
    async fn without_a_length_the_body_runs_to_the_close() {
        let (outcome, body) = answer(b"HTTP/1.0 200 OK\r\n\r\nall of it").await;
        assert_eq!(outcome.unwrap(), Outcome::Body(9));
        assert_eq!(body, b"all of it");
    }

    #[tokio::test]
    async fn a_redirect_is_handed_back_and_its_body_left_unread() {
        let raw = b"HTTP/1.1 302 Found\r\nLocation: /elsewhere\r\nContent-Length: 4\r\n\r\nbody";
        let (outcome, body) = answer(raw).await;
        assert_eq!(outcome.unwrap(), Outcome::Redirect("/elsewhere".into()));
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn anything_but_200_or_a_redirect_is_an_error() {
        for raw in [
            &b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n"[..],
            b"HTTP/1.1 206 Partial Content\r\nContent-Length: 1\r\n\r\nx",
            b"HTTP/1.1 301 Moved\r\n\r\n",
            b"SSH-2.0-OpenSSH\r\n\r\n",
        ] {
            assert!(answer(raw).await.0.is_err());
        }
    }

    #[tokio::test]
    async fn framing_that_can_be_read_two_ways_is_refused() {
        let two_lengths = b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nab";
        assert!(answer(two_lengths).await.0.is_err());
        let gzip = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\n\r\n0\r\n\r\n";
        assert!(answer(gzip).await.0.is_err());
    }

    #[tokio::test]
    async fn headers_past_their_limit_are_refused() {
        let mut raw = b"HTTP/1.1 200 OK\r\nX-Long: ".to_vec();
        raw.extend(std::iter::repeat_n(b'a', MAX_HEAD as usize));
        raw.extend_from_slice(b"\r\n\r\n");
        assert!(answer(&raw).await.0.is_err());
    }

    #[test]
    fn redirects_resolve_to_https_only() {
        let from = parse_url("https://a.example:8443/x").unwrap();
        assert_eq!(
            resolve(&from, "/y?z=1").unwrap(),
            "https://a.example:8443/y?z=1"
        );
        assert_eq!(
            resolve(&from, "//b.example/y").unwrap(),
            "https://b.example/y"
        );
        assert_eq!(
            resolve(&from, "https://c.example/y").unwrap(),
            "https://c.example/y"
        );
        assert!(resolve(&from, "http://c.example/y").is_err());
        assert!(resolve(&from, "y").is_err());
    }
}
