use tauri::http::{header, Request, Response, StatusCode};
use tauri::{Runtime, UriSchemeContext, UriSchemeResponder};

use crate::services;
use crate::services::media::Media;

/// 与 src/platform/media.ts 一致
pub const SCHEME: &str = "island";

/// 路径为 encodeURIComponent 后的 route/arg
pub fn handle<R: Runtime>(
    _ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let path = request.uri().path().to_owned();
    tauri::async_runtime::spawn_blocking(move || responder.respond(respond(&path)));
}

fn respond(path: &str) -> Response<Vec<u8>> {
    let path =
        percent_encoding::percent_decode_str(path.trim_start_matches('/')).decode_utf8_lossy();
    let (route, arg) = path.split_once('/').unwrap_or((&path, ""));
    let media = match route {
        "artwork" => services::music::artwork(arg),
        "notify" => services::notify::notify_image(arg),
        _ => None,
    };
    let builder = Response::builder();
    let response = match media {
        Some(Media { mime, bytes }) => builder.header(header::CONTENT_TYPE, mime).body(bytes),
        None => builder.status(StatusCode::NOT_FOUND).body(Vec::new()),
    };
    response.unwrap_or_default()
}
