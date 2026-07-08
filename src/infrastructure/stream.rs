use axum::body::Body;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

/// HTTP Range 헤더를 파싱한다. `(start, end)` inclusive.
pub fn parse_range_header(range: Option<&str>, file_size: u64) -> Option<(u64, u64)> {
    let range = range?;
    let spec = range.strip_prefix("bytes=")?;
    let (start_s, end_s) = spec.split_once('-')?;
    if start_s.is_empty() {
        let suffix: u64 = end_s.parse().ok()?;
        if suffix == 0 || suffix > file_size {
            return None;
        }
        return Some((file_size - suffix, file_size - 1));
    }
    let start: u64 = start_s.parse().ok()?;
    let end = if end_s.is_empty() {
        file_size - 1
    } else {
        end_s.parse().ok()?
    };
    if start > end || end >= file_size {
        return None;
    }
    Some((start, end))
}

pub async fn file_stream_response(
    mut file: File,
    file_size: u64,
    content_type: &str,
    filename: &str,
    inline: bool,
    range: Option<(u64, u64)>,
) -> Result<Response, std::io::Error> {
    let disposition = if inline {
        format!("inline; filename=\"{}\"", filename)
    } else {
        format!("attachment; filename=\"{}\"", filename)
    };

    if let Some((start, end)) = range {
        let length = end - start + 1;
        file.seek(std::io::SeekFrom::Start(start)).await?;
        let limited = file.take(length);
        let stream = ReaderStream::new(limited);
        let body = Body::from_stream(stream);

        return Ok(Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CONTENT_DISPOSITION, disposition)
            .header(header::CONTENT_LENGTH, length.to_string())
            .header(
                header::CONTENT_RANGE,
                format!("bytes {start}-{end}/{file_size}"),
            )
            .header(header::ACCEPT_RANGES, "bytes")
            .body(body)
            .unwrap());
    }

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_DISPOSITION, disposition)
        .header(header::CONTENT_LENGTH, file_size.to_string())
        .header(header::ACCEPT_RANGES, "bytes")
        .body(body)
        .unwrap())
}

pub fn range_from_headers(headers: &HeaderMap, file_size: u64) -> Option<(u64, u64)> {
    headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| parse_range_header(Some(s), file_size))
}
