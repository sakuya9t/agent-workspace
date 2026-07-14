use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::{AppError, AppState};

/// Largest attachment we store. Any file type is allowed (a PDF or a zip is a
/// perfectly good thing to hand an agent), so the size cap — not a type
/// allowlist — is what bounds this endpoint. The route also caps the transport
/// body a little higher (see the router) to give a clean 413 instead of a
/// truncated read.
pub const MAX_PASTE_BYTES: usize = 10 * 1024 * 1024;

/// Longest stem we keep from the client's filename. Long enough to stay
/// recognisable in the prompt, short enough to leave room for the uuid and the
/// extension inside any filesystem's name limit.
const MAX_STEM_LEN: usize = 48;
/// Longest extension we keep. Real ones are 1–5 chars; this only exists so a
/// pathological name can't push the filename over the limit.
const MAX_EXT_LEN: usize = 16;

/// Sniff a supported image type from the leading magic bytes and return its
/// canonical extension. The client's `Content-Type` is never trusted for this —
/// the bytes decide. Attachments no longer *have* to be images, so this is now
/// a fallback (for a clipboard blob that arrives with no filename) plus the
/// diff panel's preview check, rather than a gate.
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

/// The client's suggested filename. Advisory only — it shapes the stored name
/// but never the directory, and it is sanitised before it touches the disk.
#[derive(Deserialize)]
pub struct UploadQuery {
    name: Option<String>,
}

/// Reduce a client-supplied filename to a single safe path component.
///
/// Everything outside `[A-Za-z0-9._-]` becomes `_`, and any directory part is
/// dropped, so the result can only ever name a file *inside* the paste dir —
/// `../../etc/passwd` sanitises to `passwd`, not an escape. Returns the
/// `(stem, ext)` split, each already truncated; either may be empty.
fn safe_stem_ext(name: &str) -> (String, String) {
    // Take the basename under both separators — a Windows client may send a
    // backslash path even though this daemon runs on unix.
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let clean: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    // Leading dots would make a hidden file (and `..` a nasty one); they carry
    // no meaning for a stored attachment.
    let clean = clean.trim_start_matches('.');

    let (stem, ext) = match clean.rsplit_once('.') {
        Some((s, e)) if !e.is_empty() && e.len() <= MAX_EXT_LEN && !s.is_empty() => (s, e),
        _ => (clean, ""),
    };
    let truncate = |s: &str, n: usize| s.chars().take(n).collect::<String>();
    (truncate(stem, MAX_STEM_LEN), truncate(ext, MAX_EXT_LEN))
}

/// Store an attachment (pasted, dropped, or picked with the 📎 button) for a
/// session so its agent can read it, then hand the client back the path to
/// inject as prompt text.
///
/// This is the remote-PTY equivalent of a terminal drag-and-drop: the agent
/// (Claude Code, Codex, …) has no access to the client's clipboard or
/// filesystem, so the only way to feed it a file is a copy on this host plus a
/// path in the prompt. Any file type is accepted — an agent gets just as much
/// out of a PDF, a zip, or a CSV as it does out of a screenshot — bounded by
/// [`MAX_PASTE_BYTES`]. The file lands under
/// `<cwd>/.asm/pastes/<stem>-<uuid>.<ext>`, always reachable from the agent's
/// working directory, even under a filesystem sandbox.
///
/// The *directory* is derived entirely from the server-side session record. The
/// client only suggests a leaf name, which is sanitised to a single safe path
/// component (see [`safe_stem_ext`]) and made unique with a uuid, so it cannot
/// traverse out or clobber a previous attachment. Auth is the router's standard
/// bearer/loopback gate.
pub async fn upload(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<UploadQuery>,
    body: Bytes,
) -> Result<Json<serde_json::Value>, AppError> {
    let session = state
        .manager
        .get_session(&id)?
        .ok_or_else(|| AppError(StatusCode::NOT_FOUND, "no such session".into()))?;

    if body.is_empty() {
        return Err(AppError(StatusCode::BAD_REQUEST, "empty upload body".into()));
    }
    if body.len() > MAX_PASTE_BYTES {
        return Err(AppError(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "file too large: {} bytes (max {MAX_PASTE_BYTES})",
                body.len()
            ),
        ));
    }

    let (stem, ext) = q.name.as_deref().map(safe_stem_ext).unwrap_or_default();
    // A clipboard image arrives as a bare blob with no filename, so fall back to
    // the magic bytes; anything still unnamed gets a neutral `.bin`.
    let ext = if ext.is_empty() {
        sniff_image_ext(&body).unwrap_or("bin").to_string()
    } else {
        ext
    };
    let stem = if stem.is_empty() {
        "paste".to_string()
    } else {
        stem
    };

    // `.asm/` is the session-local scratch dir, and it ignores itself — see
    // [`crate::util::asm_dir`], which the fork brief also writes into.
    // `Path` in this module is axum's extractor, hence the fully-qualified type.
    let paste_dir = crate::util::asm_dir(std::path::Path::new(&session.working_directory))
        .map(|d| d.join("pastes"))
        .and_then(|d| std::fs::create_dir_all(&d).map(|_| d))
        .map_err(|e| {
            AppError(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("create paste dir: {e}"),
            )
        })?;

    // Keep the client's stem so the path reads meaningfully in the prompt
    // (`spec-3f2a1b9c.pdf` tells the agent more than `3f2a1b9c….pdf`), and add a
    // uuid so two uploads of the same name can't collide.
    let uniq = &Uuid::new_v4().simple().to_string()[..8];
    let filename = format!("{stem}-{uniq}.{ext}");
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
    fn sniff_declines_non_images() {
        // Non-images are still *stored* (the sniff only supplies a fallback
        // extension now); it just has nothing to say about them.
        assert_eq!(sniff_image_ext(b""), None);
        assert_eq!(sniff_image_ext(b"<html>"), None);
        assert_eq!(sniff_image_ext(&[0x00, 0x01, 0x02, 0x03]), None);
    }

    #[test]
    fn keeps_ordinary_names_intact() {
        assert_eq!(safe_stem_ext("report.pdf"), ("report".into(), "pdf".into()));
        assert_eq!(
            safe_stem_ext("my-archive_v2.tar.gz"),
            ("my-archive_v2.tar".into(), "gz".into())
        );
        assert_eq!(safe_stem_ext("notes"), ("notes".into(), String::new()));
    }

    #[test]
    fn sanitises_hostile_names_to_one_component() {
        // A traversal attempt collapses to its basename — it can only ever name
        // a file inside the paste dir.
        assert_eq!(
            safe_stem_ext("../../etc/passwd.txt"),
            ("passwd".into(), "txt".into())
        );
        assert_eq!(
            safe_stem_ext(r"C:\Users\me\evil.exe"),
            ("evil".into(), "exe".into())
        );
        // Separators, spaces, and unicode become `_`; leading dots are dropped.
        assert_eq!(
            safe_stem_ext("...hidden file;rm -rf.txt"),
            ("hidden_file_rm_-rf".into(), "txt".into())
        );
        assert_eq!(safe_stem_ext(".."), (String::new(), String::new()));
        assert_eq!(safe_stem_ext(""), (String::new(), String::new()));
    }

    #[test]
    fn truncates_pathological_lengths() {
        let (stem, ext) = safe_stem_ext(&format!("{}.{}", "a".repeat(300), "b".repeat(300)));
        assert_eq!(stem.len(), MAX_STEM_LEN);
        // An over-long extension isn't one — the whole thing stays in the stem.
        assert!(ext.is_empty());

        let (stem, ext) = safe_stem_ext(&format!("{}.pdf", "a".repeat(300)));
        assert_eq!(stem.len(), MAX_STEM_LEN);
        assert_eq!(ext, "pdf");
    }
}
