//! GitHub'a HTTPS istekleri: Windows'un kendi HTTP istemcisi (WinRT `Windows.Web.Http`), yeni paket yok.
//! Yalnız `https://api.github.com/` adresine gider. Önbellek kapalı (koşullu istekleri biz yönetiyoruz),
//! Windows'un kimlik penceresi açılmaz, yönlendirme izlenmez (token başka adrese taşınmasın). Token yalnız
//! istek başlığında durur; kayda ya da hata metnine geçmez.

use std::sync::mpsc;
use std::time::Duration;

use windows::Foundation::Uri;
use windows::Storage::Streams::UnicodeEncoding;
use windows::Web::Http::Filters::{HttpBaseProtocolFilter, HttpCacheReadBehavior, HttpCacheWriteBehavior};
use windows::Web::Http::{HttpClient, HttpMethod, HttpRequestMessage, HttpResponseMessage, HttpStringContent};
use windows::core::HSTRING;

use super::api::API;

/// Bir isteğin en uzun süresi.
const TIMEOUT: Duration = Duration::from_secs(20);

pub struct Response {
    pub status: u16,
    pub body: String,
    /// `Last-Modified`, `X-Poll-Interval`, `X-OAuth-Scopes`, `X-RateLimit-Remaining` (yalnız bunlar).
    pub last_modified: Option<String>,
    pub poll_interval: Option<u64>,
    pub scopes: Option<String>,
    pub rate_remaining: Option<u64>,
}

pub struct Client {
    client: HttpClient,
}

const NET_ERROR: &str = "GitHub'a ulaşılamıyor.";

impl Client {
    pub fn new() -> Result<Self, String> {
        let make = || -> windows::core::Result<HttpClient> {
            let filter = HttpBaseProtocolFilter::new()?;
            filter.SetAllowUI(false)?;
            // Yönlendirme izlenmez: Authorization başlığı api.github.com dışına gitmesin.
            filter.SetAllowAutoRedirect(false)?;
            let cache = filter.CacheControl()?;
            cache.SetReadBehavior(HttpCacheReadBehavior::NoCache)?;
            cache.SetWriteBehavior(HttpCacheWriteBehavior::NoCache)?;
            HttpClient::Create(&filter)
        };
        make().map(|client| Self { client }).map_err(|_| NET_ERROR.to_string())
    }

    /// `path` "/notifications" gibi; `body` varsa JSON gönderilir.
    pub fn send(&self, method: &str, path: &str, token: &str, body: Option<&str>, if_modified_since: Option<&str>) -> Result<Response, String> {
        if !path.starts_with('/') || path.contains("://") {
            return Err("Geçersiz adres.".into());
        }
        // Başlık doğrulamasız eklendiği için satır sonu ya da boşluk içeren token hiç gönderilmez.
        if token.is_empty() || token.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err("Token geçersiz görünüyor.".into());
        }
        let run = || -> windows::core::Result<Option<Response>> {
            let method = match method {
                "POST" => HttpMethod::Post()?,
                "PATCH" => HttpMethod::Patch()?,
                _ => HttpMethod::Get()?,
            };
            let uri = Uri::CreateUri(&HSTRING::from(format!("{API}{path}")))?;
            let request = HttpRequestMessage::Create(&method, &uri)?;
            let headers = request.Headers()?;
            let add = |name: &str, value: &str| headers.TryAppendWithoutValidation(&HSTRING::from(name), &HSTRING::from(value)).map(|_| ());
            add("Authorization", &format!("Bearer {token}"))?;
            add("Accept", "application/vnd.github+json")?;
            add("X-GitHub-Api-Version", "2022-11-28")?;
            add("User-Agent", "Zodyvel")?;
            if let Some(since) = if_modified_since {
                add("If-Modified-Since", since)?;
            }
            if let Some(body) = body {
                let content = HttpStringContent::CreateFromStringWithEncodingAndMediaType(&HSTRING::from(body), UnicodeEncoding::Utf8, &HSTRING::from("application/json"))?;
                request.SetContent(&content)?;
            }
            // Zaman aşımlı bekleme: süre dolarsa istek iptal edilir.
            let op = self.client.SendRequestAsync(&request)?;
            let (tx, rx) = mpsc::channel();
            op.when(move |result| {
                let _ = tx.send(result);
            })?;
            let Ok(response) = rx.recv_timeout(TIMEOUT) else {
                let _ = op.Cancel();
                return Ok(None);
            };
            Ok(Some(read(&response?)?))
        };
        match run() {
            Ok(Some(r)) => Ok(r),
            _ => Err(NET_ERROR.into()),
        }
    }
}

fn read(response: &HttpResponseMessage) -> windows::core::Result<Response> {
    let status = response.StatusCode()?.0 as u16;
    let headers = response.Headers()?;
    let content = response.Content()?;
    let content_headers = content.Headers()?;
    let header = |name: &str| -> Option<String> {
        let name = HSTRING::from(name);
        headers.Lookup(&name).or_else(|_| content_headers.Lookup(&name)).ok().map(|v| v.to_string())
    };
    let body = content.ReadAsStringAsync()?.join().map(|s| s.to_string()).unwrap_or_default();
    Ok(Response {
        status,
        body,
        last_modified: header("Last-Modified"),
        poll_interval: header("X-Poll-Interval").and_then(|v| v.trim().parse().ok()),
        scopes: header("X-OAuth-Scopes"),
        rate_remaining: header("X-RateLimit-Remaining").and_then(|v| v.trim().parse().ok()),
    })
}
