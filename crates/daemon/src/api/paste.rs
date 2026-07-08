use std::path::PathBuf;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::json;
use uuid::Uuid;

use super::{AppError, AppState};

/// Largest paste payload we store. Matches Claude Code's per-image limit so we
/// never persist something the agent will reject on read. The route also caps
/// the transport body a little higher (see the router) to give a clean 413
/// instead of a truncated read.
pub const MAX_PASTE_BYTES: usize = 5 * 1024 * 1024;

/// Sniff a supported image type from the leading magic bytes and return its
/// canonical extension. The client's `Content-Type` is never trusted for this —
/// the bytes decide, so a mislabelled or hostile upload cannot smuggle a
/// non-image onto the host disk.
fn sniff_image_ext(b: &[u8]) -> Option<&'static str> {
    const PNG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if b.len() >= 8 && b[..8] == PNG {
        Some("png")
    } else if b.len() >= 3 && b[..3] == [0xFF, 0xD8, 0xFF] {
        Some("jpg")
    } else if b.len() >= 6 && (&b[..6] == b"GIF87a" || &b[..6] == b"GIF89a") {
        Some("gif")
    } else if b.len() >= 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

/// MIME type of a supported image, sniffed from the same magic bytes as
/// [`sniff_image_ext`]. Shared with the diff panel's preview endpoint so both
/// the paste and preview paths trust the bytes, never the filename.
pub(crate) fn sniff_image_mime(b: &[u8]) -> Option<&'static str> {
    Some(match sniff_image_ext(b)? {
        "png" => "image/png",
        "jpg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return None,
    })
}

/// Store a pasted/dropped image for a session so its agent can read it, then
/// hand the client back the path to inject as prompt text.
///
/// This is the remote-PTY equivalent of a terminal drag-and-drop: the agent
/// (Claude Code, Codex, …) has no access to the client's clipboard, so the only
/// way to feed it an image is a file on this host plus a path in the prompt.
/// The image lands under `<cwd>/.asm/pastes/<uuid>.<ext>` — always reachable
/// from the agent's working directory, even under a filesystem sandbox.
///
/// The destination is derived entirely from the server-side session record;
/// the client supplies only bytes, so it cannot influence the path (no
/// traversal). Auth is the router's standard bearer/loopback gate.
pub async fn upload(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<serde_json::Value>, AppError> {
    let session = state
        .manager
        .get_session(&id)?
        .ok_or_else(|| AppError(StatusCode::NOT_FOUND, "no such session".into()))?;

    if body.is_empty() {
        return Err(AppError(StatusCode::BAD_REQUEST, "empty image body".into()));
    }
    if body.len() > MAX_PASTE_BYTES {
        return Err(AppError(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "image too large: {} bytes (max {MAX_PASTE_BYTES})",
                body.len()
            ),
        ));
    }
    let ext = sniff_image_ext(&body).ok_or_else(|| {
        AppError(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported image type (expected PNG, JPEG, GIF, or WebP)".into(),
        )
    })?;

    let asm_dir = PathBuf::from(&session.working_directory).join(".asm");
    let paste_dir = asm_dir.join("pastes");
    std::fs::create_dir_all(&paste_dir).map_err(|e| {
        AppError(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("create paste dir: {e}"),
        )
    })?;

    // Keep pastes out of version control without touching tracked files or the
    // repo's git config: a self-contained `*` ignore inside `.asm/` covers
    // every worktree layout. Best-effort — a failure here doesn't fail the
    // paste (the file is still usable), it just risks a dirty status entry.
    let _ = std::fs::write(asm_dir.join(".gitignore"), "*\n");

    let filename = format!("{}.{ext}", Uuid::new_v4().simple());
    let abs = paste_dir.join(&filename);
    std::fs::write(&abs, &body).map_err(|e| {
        AppError(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("write paste: {e}"),
        )
    })?;

    Ok(Json(json!({
        "ok": true,
        // Absolute path on the host; the relative form is what the client
        // injects (the agent runs in `cwd`, so it resolves and reads tidier).
        "path": abs.to_string_lossy(),
        "relative_path": format!(".asm/pastes/{filename}"),
        "filename": filename,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_known_image_types() {
        assert_eq!(
            sniff_image_ext(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0]),
            Some("png")
        );
        assert_eq!(sniff_image_ext(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(sniff_image_ext(b"GIF89a...."), Some("gif"));
        let mut webp = b"RIFF\0\0\0\0WEBPVP8 ".to_vec();
        webp.truncate(16);
        assert_eq!(sniff_image_ext(&webp), Some("webp"));
    }

    #[test]
    fn rejects_non_images() {
        assert_eq!(sniff_image_ext(b""), None);
        assert_eq!(sniff_image_ext(b"<html>"), None);
        assert_eq!(sniff_image_ext(&[0x00, 0x01, 0x02, 0x03]), None);
    }
}
