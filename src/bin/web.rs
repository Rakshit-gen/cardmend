//! cardmend in the browser: drop address book exports, review each group of
//! duplicates card by card, and download the clean file.
//!
//!     cargo run --release --features web --bin cardmend-web
//!
//! Binds to 127.0.0.1 only. The files you drop stay in this process's memory
//! and are never written anywhere; closing it forgets them.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::extract::{DefaultBodyLimit, Path as UrlPath, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use cardmend::contact::{Contact, Photo};
use cardmend::import::Book;
use cardmend::merge::{self, Choices};
use cardmend::{Analysis, analyze, normalize, write};
use clap::Parser;
use phonenumber::country::Id;
use serde::Deserialize;
use serde_json::{Value, json};
use tower_http::services::{ServeDir, ServeFile};

#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Port on 127.0.0.1 to listen on.
    #[arg(long, default_value_t = 7575)]
    port: u16,
    /// Country for numbers saved without a country code, like IN or GB.
    /// Defaults to the one in your locale.
    #[arg(long)]
    region: Option<String>,
    /// Serve the UI from this directory instead of the copy built into the
    /// binary. Handy while working on the UI.
    #[arg(long)]
    static_dir: Option<PathBuf>,
}

struct App {
    region: Id,
    session: Mutex<Session>,
}

#[derive(Default)]
struct Session {
    book: Book,
    /// One hash per file added, to refuse the same file twice and to give
    /// the page a key for saving review progress.
    hashes: Vec<u64>,
    analysis: Option<Arc<Analysis>>,
}

impl Session {
    /// Identifies this exact set of files, so decisions saved in the browser
    /// only come back for the same input.
    fn fingerprint(&self) -> String {
        let mut h = Fnv::default();
        for x in &self.hashes {
            h.eat(&x.to_le_bytes());
        }
        format!("{:016x}", h.0)
    }
}

/// FNV-1a: stable across builds, unlike std's hasher, which matters because
/// the fingerprint is stored in the browser.
struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Fnv(0xcbf29ce484222325)
    }
}

impl Fnv {
    fn eat(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x100000001b3);
        }
    }
}

struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

fn bad(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.into())
}

/// A contact as the page needs it: the photo becomes a link so a big book
/// with photos doesn't arrive as one huge JSON document.
fn contact_json(c: &Contact) -> Value {
    let mut v = serde_json::to_value(c).expect("contacts serialise");
    v["display"] = json!(c.display_name());
    if c.photo.is_some() {
        v["photo"] = json!(format!("/api/photo/{}", c.id));
    }
    v
}

fn describe(app: &App, s: &Session) -> Value {
    let Some(a) = &s.analysis else {
        return json!({ "state": "empty", "region": app.region.as_ref() });
    };
    let c = &s.book.contacts;
    // Only contacts the page can show: group members, those with a
    // problem, and those sharing a number or email. The rest go straight
    // to the output unchanged.
    let mut shown: Vec<usize> = a
        .groups
        .iter()
        .flat_map(|g| g.members.iter().copied())
        .chain(a.problems.no_name.iter().copied())
        .chain(a.problems.no_country.iter().map(|x| x.0))
        .chain(a.problems.empty.iter().copied())
        .chain(a.shared.iter().flat_map(|x| x.contacts.iter().copied()))
        .collect();
    shown.sort_unstable();
    shown.dedup();
    let groups: Vec<Value> = a
        .groups
        .iter()
        .map(|g| {
            json!({
                "tier": g.tier,
                "score": (g.score * 100.0).round() / 100.0,
                "members": g.members,
                "pairs": g.pairs.iter().map(|&p| {
                    let p = &a.pairs[p];
                    json!({
                        "a": p.a,
                        "b": p.b,
                        "score": (p.score * 100.0).round() / 100.0,
                        "why": p.evidence,
                    })
                }).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({
        "state": "ready",
        "fingerprint": s.fingerprint(),
        "region": app.region.as_ref(),
        "files": s.book.files,
        "issues": s.book.issues,
        "total": c.len(),
        "contacts": shown.iter().map(|&i| (i.to_string(), contact_json(&c[i]))).collect::<serde_json::Map<_, _>>(),
        "groups": groups,
        "shared": a.shared,
        "problems": a.problems,
    })
}

async fn get_analysis(State(app): State<Arc<App>>) -> Json<Value> {
    let s = app.session.lock().unwrap();
    Json(describe(&app, &s))
}

/// The page sends each dropped file as the raw body, with its name in a
/// header, so nothing needs multipart parsing.
async fn add_file(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, ApiError> {
    let name = headers
        .get("x-filename")
        .and_then(|v| v.to_str().ok())
        .map(percent_decode)
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "contacts".into());
    let mut h = Fnv::default();
    h.eat(&body);
    let app2 = app.clone();
    tokio::task::spawn_blocking(move || {
        let mut s = app2.session.lock().unwrap();
        if s.hashes.contains(&h.0) {
            return Err(ApiError(
                StatusCode::CONFLICT,
                format!("{name} is already added; each file only needs dropping once"),
            ));
        }
        s.hashes.push(h.0);
        s.book.add(&name, &body);
        let a = analyze(&s.book.contacts, app2.region, &[]);
        s.analysis = Some(Arc::new(a));
        Ok(Json(describe(&app2, &s)))
    })
    .await
    .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
}

/// Browsers can only send ASCII in headers, so the page percent-encodes
/// the file name.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |x: u8| (x as char).to_digit(16);
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            out.push((h * 16 + l) as u8);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

async fn clear(State(app): State<Arc<App>>) -> Json<Value> {
    let mut s = app.session.lock().unwrap();
    *s = Session::default();
    Json(describe(&app, &s))
}

async fn photo(
    State(app): State<Arc<App>>,
    UrlPath(id): UrlPath<usize>,
) -> Result<Response, ApiError> {
    let s = app.session.lock().unwrap();
    match s.book.contacts.get(id).and_then(|c| c.photo.as_ref()) {
        Some(Photo::Data { mime, bytes }) => {
            Ok(([(header::CONTENT_TYPE, mime.clone())], bytes.clone()).into_response())
        }
        Some(Photo::Uri(u)) => {
            Ok((StatusCode::FOUND, [(header::LOCATION, u.clone())]).into_response())
        }
        None => Err(ApiError(StatusCode::NOT_FOUND, "no photo".into())),
    }
}

#[derive(Deserialize)]
struct MergeRequest {
    members: Vec<usize>,
    #[serde(default)]
    choices: Choices,
}

fn members<'a>(s: &'a Session, ids: &[usize]) -> Result<Vec<&'a Contact>, ApiError> {
    if ids.is_empty() {
        return Err(bad("pick at least one contact"));
    }
    ids.iter()
        .map(|&i| {
            s.book.contacts.get(i).ok_or_else(|| {
                ApiError(
                    StatusCode::CONFLICT,
                    "the files changed since this page loaded; reload it".into(),
                )
            })
        })
        .collect()
}

/// What a group looks like merged with the given picks.
async fn preview(
    State(app): State<Arc<App>>,
    Json(req): Json<MergeRequest>,
) -> Result<Json<Value>, ApiError> {
    let s = app.session.lock().unwrap();
    let m = merge::merge(&members(&s, &req.members)?, app.region, &req.choices);
    Ok(Json(json!({
        "contact": contact_json(&m.contact),
        "choices": m.choices,
        "alternatives": m.alternatives,
    })))
}

#[derive(Deserialize)]
struct ExportRequest {
    merges: Vec<MergeRequest>,
}

async fn export(
    State(app): State<Arc<App>>,
    Json(req): Json<ExportRequest>,
) -> Result<Response, ApiError> {
    let s = app.session.lock().unwrap();
    if s.book.contacts.is_empty() {
        return Err(bad(
            "there's nothing to export yet; drop a contacts file first",
        ));
    }
    for m in &req.merges {
        members(&s, &m.members)?;
    }
    let merges: Vec<(Vec<usize>, Choices)> = req
        .merges
        .into_iter()
        .map(|m| (m.members, m.choices))
        .collect();
    let (clean, summary) = merge::apply(&s.book.contacts, &merges, app.region);
    Ok((
        [
            (
                header::CONTENT_TYPE,
                "text/vcard; charset=utf-8".to_string(),
            ),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"contacts-clean.vcf\"".to_string(),
            ),
            (
                header::HeaderName::from_static("x-cardmend-summary"),
                serde_json::to_string(&summary).unwrap(),
            ),
        ],
        write::write_all(&clean),
    )
        .into_response())
}

#[derive(rust_embed::RustEmbed)]
#[folder = "web/dist"]
#[allow_missing = true]
struct Ui;

async fn embedded_ui(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let (file, path) = match Ui::get(path) {
        Some(f) if !path.is_empty() => (f, path),
        _ => match Ui::get("index.html") {
            Some(f) => (f, "index.html"),
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    "This build has no UI. Run `pnpm --dir web build` and rebuild, or pass --static-dir.",
                )
                    .into_response();
            }
        },
    };
    let cache = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    (
        [
            (header::CONTENT_TYPE, file.metadata.mimetype().to_string()),
            (header::CACHE_CONTROL, cache.to_string()),
        ],
        file.data,
    )
        .into_response()
}

fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/api/analysis", get(get_analysis))
        .route("/api/files", post(add_file).delete(clear))
        .route("/api/photo/{id}", get(photo))
        .route("/api/preview", post(preview))
        .route("/api/export", post(export))
        // A phone's full export with photos can run to tens of megabytes.
        .layer(DefaultBodyLimit::max(256 * 1024 * 1024))
        .with_state(app)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let region = match &cli.region {
        Some(r) => normalize::region(r).ok_or_else(|| {
            anyhow::anyhow!("{r} isn't a two-letter country code like IN, GB or US")
        })?,
        None => normalize::default_region(),
    };
    let state = Arc::new(App {
        region,
        session: Mutex::new(Session::default()),
    });
    let app = router(state);
    let app = match &cli.static_dir {
        Some(dir) => app
            .fallback_service(ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html")))),
        None => app.fallback(embedded_ui),
    };

    let addr = SocketAddr::from(([127, 0, 0, 1], cli.port));
    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
        anyhow::anyhow!("can't listen on {addr}: {e}. Is cardmend-web already running? Try --port.")
    })?;
    println!(
        "cardmend is running at http://{addr} (numbers without a country code read as {})",
        region.as_ref()
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    const BOOK: &str = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Priya Shah\r\nN:Shah;Priya;;;\r\n\
        TEL;TYPE=CELL:+91 98200 12345\r\nEND:VCARD\r\n\
        BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Priya Shah\r\nN:Shah;Priya;;;\r\n\
        TEL;TYPE=CELL:098200 12345\r\nEMAIL:priya@example.com\r\nEND:VCARD\r\n\
        BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Rahul Mehta\r\nN:Mehta;Rahul;;;\r\nEND:VCARD\r\n";

    fn app() -> Router {
        router(Arc::new(App {
            region: Id::IN,
            session: Mutex::new(Session::default()),
        }))
    }

    async fn call(app: &Router, req: Request<Body>) -> (StatusCode, Vec<u8>) {
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        (
            status,
            res.into_body().collect().await.unwrap().to_bytes().to_vec(),
        )
    }

    fn upload(name: &str, body: &str) -> Request<Body> {
        Request::post("/api/files")
            .header("x-filename", name)
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn upload_review_and_export() {
        let app = app();
        let (status, body) = call(&app, upload("phone%20backup.vcf", BOOK)).await;
        assert_eq!(status, StatusCode::OK);
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["state"], "ready");
        assert_eq!(v["files"][0][0], "phone backup.vcf");
        assert_eq!(v["total"], 3);
        assert_eq!(v["groups"][0]["members"], json!([0, 1]));
        assert_eq!(v["groups"][0]["tier"], "sure");

        // The same file again is refused rather than doubling every contact.
        let (status, _) = call(&app, upload("again.vcf", BOOK)).await;
        assert_eq!(status, StatusCode::CONFLICT);

        let req = Request::post("/api/export")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"merges":[{"members":[0,1]}]}"#))
            .unwrap();
        let (status, body) = call(&app, req).await;
        assert_eq!(status, StatusCode::OK);
        let text = String::from_utf8(body).unwrap();
        assert_eq!(text.matches("BEGIN:VCARD").count(), 2);
        assert!(text.contains("priya@example.com"));
    }

    #[tokio::test]
    async fn preview_honours_choices_and_rejects_unknown_ids() {
        let app = app();
        call(&app, upload("a.vcf", BOOK)).await;
        let req = Request::post("/api/preview")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"members":[0,1],"choices":{"name":1}}"#))
            .unwrap();
        let (status, body) = call(&app, req).await;
        assert_eq!(status, StatusCode::OK);
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["choices"]["name"], 1);
        assert_eq!(v["contact"]["phones"].as_array().unwrap().len(), 1);

        let req = Request::post("/api/preview")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"members":[0,99]}"#))
            .unwrap();
        assert_eq!(call(&app, req).await.0, StatusCode::CONFLICT);
    }

    #[test]
    fn decodes_file_names() {
        assert_eq!(
            percent_decode("Kontakte%20M%C3%BCller.vcf"),
            "Kontakte Müller.vcf"
        );
        assert_eq!(percent_decode("100%"), "100%");
    }
}
