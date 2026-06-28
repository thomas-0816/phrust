use php_runtime::{RuntimeHttpRequestContext, RuntimeUploadedFile};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static UPLOAD_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MultipartConfig {
    pub upload_temp_dir: PathBuf,
    pub max_upload_files: usize,
    pub max_upload_file_bytes: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct MultipartStats {
    pub uploads_total: u64,
    pub upload_bytes_accepted: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MultipartError {
    Malformed,
    TooManyFiles,
    FileTooLarge,
    Storage,
}

#[derive(Debug)]
struct PartDisposition {
    name: String,
    filename: Option<String>,
}

pub(crate) fn multipart_boundary(
    content_type: Option<&str>,
) -> Result<Option<String>, MultipartError> {
    let Some(content_type) = content_type else {
        return Ok(None);
    };
    let mut parts = content_type.split(';');
    let Some(media_type) = parts.next() else {
        return Ok(None);
    };
    if !media_type
        .trim()
        .eq_ignore_ascii_case("multipart/form-data")
    {
        return Ok(None);
    }
    for parameter in parts {
        let Some((name, value)) = parameter.split_once('=') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("boundary") {
            let boundary = unquote_parameter(value.trim());
            if boundary.is_empty()
                || boundary.len() > 200
                || boundary.bytes().any(|byte| matches!(byte, b'\r' | b'\n'))
            {
                return Err(MultipartError::Malformed);
            }
            return Ok(Some(boundary));
        }
    }
    Err(MultipartError::Malformed)
}

pub(crate) fn parse_multipart_into_context(
    context: &mut RuntimeHttpRequestContext,
    body: &[u8],
    boundary: &str,
    config: &MultipartConfig,
) -> Result<MultipartStats, MultipartError> {
    let original_upload_count = context.uploaded_files.len();
    match parse_multipart_inner(context, body, boundary, config) {
        Ok(stats) => Ok(stats),
        Err(error) => {
            cleanup_uploaded_files(&context.uploaded_files[original_upload_count..]);
            context.uploaded_files.truncate(original_upload_count);
            Err(error)
        }
    }
}

pub(crate) fn cleanup_uploaded_files(files: &[RuntimeUploadedFile]) {
    for file in files {
        let _ = fs::remove_file(&file.temp_path);
    }
}

pub(crate) fn sanitize_client_filename(value: &str) -> String {
    let without_nuls = value.replace('\0', "");
    without_nuls
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .to_string()
}

fn parse_multipart_inner(
    context: &mut RuntimeHttpRequestContext,
    body: &[u8],
    boundary: &str,
    config: &MultipartConfig,
) -> Result<MultipartStats, MultipartError> {
    let marker = format!("--{boundary}");
    let marker = marker.as_bytes();
    if !body.starts_with(marker) {
        return Err(MultipartError::Malformed);
    }

    fs::create_dir_all(&config.upload_temp_dir).map_err(|_| MultipartError::Storage)?;

    let next_boundary = {
        let mut value = Vec::with_capacity(marker.len() + 2);
        value.extend_from_slice(b"\r\n");
        value.extend_from_slice(marker);
        value
    };
    let mut stats = MultipartStats::default();
    let mut file_count = 0usize;
    let mut offset = marker.len();

    loop {
        if body.get(offset..offset + 2) == Some(b"--") {
            return Ok(stats);
        }
        if body.get(offset..offset + 2) != Some(b"\r\n") {
            return Err(MultipartError::Malformed);
        }
        offset += 2;

        let header_end = find_bytes(&body[offset..], b"\r\n\r\n")
            .map(|index| offset + index)
            .ok_or(MultipartError::Malformed)?;
        let headers = parse_part_headers(&body[offset..header_end])?;
        offset = header_end + 4;

        let data_end = find_bytes(&body[offset..], &next_boundary)
            .map(|index| offset + index)
            .ok_or(MultipartError::Malformed)?;
        let data = &body[offset..data_end];
        offset = data_end + next_boundary.len();

        let disposition = part_disposition(&headers)?;
        if let Some(filename) = disposition.filename {
            if file_count >= config.max_upload_files {
                return Err(MultipartError::TooManyFiles);
            }
            if data.len() > config.max_upload_file_bytes {
                return Err(MultipartError::FileTooLarge);
            }
            file_count += 1;
            let temp_path = write_upload_temp_file(&config.upload_temp_dir, data)
                .map_err(|_| MultipartError::Storage)?;
            let content_type = header_lookup(&headers, "content-type")
                .unwrap_or("")
                .to_string();
            context.uploaded_files.push(RuntimeUploadedFile {
                field_name: disposition.name,
                client_filename: sanitize_client_filename(&filename),
                content_type,
                temp_path: temp_path.to_string_lossy().into_owned(),
                error: 0,
                size: data.len() as u64,
            });
            stats.uploads_total += 1;
            stats.upload_bytes_accepted = stats
                .upload_bytes_accepted
                .saturating_add(u64::try_from(data.len()).unwrap_or(u64::MAX));
        } else {
            context
                .parsed_post
                .push((disposition.name, String::from_utf8_lossy(data).into_owned()));
        }
    }
}

fn parse_part_headers(input: &[u8]) -> Result<Vec<(String, String)>, MultipartError> {
    let text = std::str::from_utf8(input).map_err(|_| MultipartError::Malformed)?;
    let mut headers = Vec::new();
    for line in text.split("\r\n") {
        let (name, value) = line.split_once(':').ok_or(MultipartError::Malformed)?;
        headers.push((name.trim().to_string(), value.trim().to_string()));
    }
    Ok(headers)
}

fn part_disposition(headers: &[(String, String)]) -> Result<PartDisposition, MultipartError> {
    let value = header_lookup(headers, "content-disposition").ok_or(MultipartError::Malformed)?;
    let mut parts = value.split(';');
    let Some(disposition_type) = parts.next() else {
        return Err(MultipartError::Malformed);
    };
    if !disposition_type.trim().eq_ignore_ascii_case("form-data") {
        return Err(MultipartError::Malformed);
    }
    let mut name = None;
    let mut filename = None;
    for parameter in parts {
        let Some((key, value)) = parameter.split_once('=') else {
            continue;
        };
        let value = unquote_parameter(value.trim());
        match key.trim().to_ascii_lowercase().as_str() {
            "name" => name = Some(value),
            "filename" => filename = Some(value),
            _ => {}
        }
    }
    Ok(PartDisposition {
        name: name.ok_or(MultipartError::Malformed)?,
        filename,
    })
}

fn header_lookup<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn unquote_parameter(value: &str) -> String {
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value[1..value.len() - 1].replace("\\\"", "\"")
    } else {
        value.to_string()
    }
}

fn write_upload_temp_file(dir: &Path, data: &[u8]) -> std::io::Result<PathBuf> {
    for _ in 0..100 {
        let id = UPLOAD_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!("upload-{}-{id}", std::process::id()));
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        file.write_all(data)?;
        return Ok(path);
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate unique upload temp file",
    ))
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_detection_handles_multipart_and_other_content_types() {
        assert_eq!(
            multipart_boundary(Some("multipart/form-data; boundary=abc")).unwrap(),
            Some("abc".to_string())
        );
        assert_eq!(
            multipart_boundary(Some("multipart/form-data; boundary=\"quoted\"")).unwrap(),
            Some("quoted".to_string())
        );
        assert_eq!(
            multipart_boundary(Some("application/x-www-form-urlencoded")).unwrap(),
            None
        );
        assert_eq!(
            multipart_boundary(Some("multipart/form-data")),
            Err(MultipartError::Malformed)
        );
    }

    #[test]
    fn filename_sanitization_strips_paths_and_nuls() {
        assert_eq!(sanitize_client_filename("avatar.png"), "avatar.png");
        assert_eq!(sanitize_client_filename("../avatar.png"), "avatar.png");
        assert_eq!(
            sanitize_client_filename("C:\\tmp\\avatar\0.png"),
            "avatar.png"
        );
    }

    #[test]
    fn multipart_parser_extracts_fields_and_files() {
        let temp_dir = unique_temp_dir();
        let mut context = RuntimeHttpRequestContext::new(
            "POST",
            "example.test",
            "/upload.php",
            "/upload.php",
            "/srv/upload.php",
            "/srv",
        );
        let body = b"--BOUNDARY\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nHello\r\n--BOUNDARY\r\nContent-Disposition: form-data; name=\"avatar\"; filename=\"../me.png\"\r\nContent-Type: image/png\r\n\r\nPNGDATA\r\n--BOUNDARY--";
        let stats = parse_multipart_into_context(
            &mut context,
            body,
            "BOUNDARY",
            &MultipartConfig {
                upload_temp_dir: temp_dir.clone(),
                max_upload_files: 2,
                max_upload_file_bytes: 64,
            },
        )
        .unwrap();

        assert_eq!(stats.uploads_total, 1);
        assert_eq!(stats.upload_bytes_accepted, 7);
        assert_eq!(
            context.parsed_post,
            vec![("title".to_string(), "Hello".to_string())]
        );
        assert_eq!(context.uploaded_files.len(), 1);
        let upload = &context.uploaded_files[0];
        assert_eq!(upload.field_name, "avatar");
        assert_eq!(upload.client_filename, "me.png");
        assert_eq!(upload.content_type, "image/png");
        assert_eq!(upload.error, 0);
        assert_eq!(upload.size, 7);
        assert_eq!(std::fs::read(&upload.temp_path).unwrap(), b"PNGDATA");

        cleanup_uploaded_files(&context.uploaded_files);
        std::fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn multipart_parser_rejects_file_limits_and_cleans_partial_uploads() {
        let temp_dir = unique_temp_dir();
        let mut context = RuntimeHttpRequestContext::new(
            "POST",
            "example.test",
            "/upload.php",
            "/upload.php",
            "/srv/upload.php",
            "/srv",
        );
        let body = b"--BOUNDARY\r\nContent-Disposition: form-data; name=\"first\"; filename=\"one.txt\"\r\n\r\none\r\n--BOUNDARY\r\nContent-Disposition: form-data; name=\"second\"; filename=\"two.txt\"\r\n\r\ntwo\r\n--BOUNDARY--";

        assert_eq!(
            parse_multipart_into_context(
                &mut context,
                body,
                "BOUNDARY",
                &MultipartConfig {
                    upload_temp_dir: temp_dir.clone(),
                    max_upload_files: 1,
                    max_upload_file_bytes: 64,
                },
            ),
            Err(MultipartError::TooManyFiles)
        );
        assert!(context.uploaded_files.is_empty());
        assert_eq!(std::fs::read_dir(&temp_dir).unwrap().count(), 0);

        std::fs::remove_dir_all(temp_dir).unwrap();
    }

    fn unique_temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "phrust-server-multipart-test-{}-{}",
            std::process::id(),
            UPLOAD_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        path
    }
}
