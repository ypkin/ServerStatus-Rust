use axum::{
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../web"]
#[prefix = "/"]
pub struct Asset;

pub async fn index_handler() -> impl IntoResponse {
    static_handler(&"/index.html".parse::<Uri>().unwrap())
}

#[allow(unused)]
#[allow(clippy::unused_async)]
pub async fn admin_index_handler() -> impl IntoResponse {
    static_handler(&"/admin.html".parse::<Uri>().unwrap())
}

pub fn static_handler(uri: &Uri) -> impl IntoResponse {
    let path = uri.path().to_string();
    StaticFile(path)
}

pub struct StaticFile<T>(pub T);

impl<T> IntoResponse for StaticFile<T>
where
    T: Into<String>,
{
    fn into_response(self) -> Response {
        let path = self.0.into();
        match Asset::get(path.as_str()) {
            Some(content) => {
                let mime = mime_guess::from_path(&path).first_or_octet_stream();
                // Batch 4: the HTML entry points are never cached. They
                // carry the overlay's ?v= URL, and a stale cached copy
                // of index.html is exactly how users kept running a
                // pre-v7 overlay (in-cell detail + NaN boot time) long
                // after v7 shipped. Versioned JS/CSS keep heuristic
                // caching; only the entry HTML is revalidated.
                if path == "/index.html" || path == "/admin.html" {
                    (
                        [
                            (header::CONTENT_TYPE, mime.as_ref()),
                            (header::CACHE_CONTROL, "no-cache"),
                        ],
                        content.data,
                    )
                        .into_response()
                } else {
                    ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
                }
            }
            None => (StatusCode::NOT_FOUND, "404").into_response(),
        }
    }
}
