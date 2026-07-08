use encoding_rs::{EUC_KR, UTF_16BE, UTF_16LE};
use std::time::Duration;

const SUBTITLE_EXTS: &[&str] = &["vtt", "srt", "smi", "ass", "ssa"];

pub fn subtitle_extension(name: &str) -> Option<&'static str> {
    let ext = name.rsplit('.').next()?.to_lowercase();
    SUBTITLE_EXTS
        .iter()
        .find(|e| **e == ext)
        .copied()
}

pub fn file_stem(name: &str) -> &str {
    name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(name)
}

/// `movie.mp4` ↔ `movie.smi`, `movie.ko.srt` 등 같은 폴더 자막 매칭 (대소문자 무시)
pub fn matches_video_subtitle(video_name: &str, subtitle_name: &str) -> bool {
    let Some(_) = subtitle_extension(subtitle_name) else {
        return false;
    };
    let video_base = file_stem(video_name).to_lowercase();
    let sub_stem = file_stem(subtitle_name).to_lowercase();
    sub_stem == video_base || sub_stem.starts_with(&format!("{video_base}."))
}

pub fn subtitle_label(name: &str) -> String {
    file_stem(name).to_string()
}

/// 자막 파일 바이트를 UTF-8 문자열로 디코딩한다. (UTF-8/UTF-16 BOM, EUC-KR·CP949)
pub fn decode_subtitle_bytes(raw: &[u8]) -> Result<String, String> {
    if raw.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(raw[3..].to_vec()).map_err(|e| format!("UTF-8 decode: {e}"));
    }
    if raw.starts_with(&[0xFF, 0xFE]) {
        let (s, _, _) = UTF_16LE.decode(&raw[2..]);
        return Ok(s.into_owned());
    }
    if raw.starts_with(&[0xFE, 0xFF]) {
        let (s, _, _) = UTF_16BE.decode(&raw[2..]);
        return Ok(s.into_owned());
    }
    if let Ok(s) = std::str::from_utf8(raw) {
        return Ok(s.to_string());
    }
    let (decoded, _, had_errors) = EUC_KR.decode(raw);
    if had_errors {
        return Err("지원하지 않는 자막 인코딩입니다.".into());
    }
    Ok(decoded.into_owned())
}

pub fn to_vtt(content: &str, ext: &str) -> Result<String, String> {
    match ext {
        "vtt" => Ok(normalize_vtt(content)),
        "srt" => srt_to_vtt(content),
        "smi" => smi_to_vtt(content),
        "ass" | "ssa" => ass_to_vtt(content),
        _ => Err(format!("unsupported subtitle format: {ext}")),
    }
}

fn normalize_vtt(content: &str) -> String {
    let trimmed = content.trim_start_matches('\u{feff}');
    if trimmed.starts_with("WEBVTT") {
        trimmed.to_string()
    } else {
        format!("WEBVTT\n\n{trimmed}")
    }
}

fn srt_to_vtt(content: &str) -> Result<String, String> {
    let mut out = String::from("WEBVTT\n\n");
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let blocks: Vec<&str> = normalized.split("\n\n").collect();

    for block in blocks {
        let lines: Vec<&str> = block.lines().filter(|l| !l.trim().is_empty()).collect();
        if lines.len() < 2 {
            continue;
        }
        let time_line = if lines[0].contains("-->") {
            lines[0]
        } else if lines.len() >= 2 && lines[1].contains("-->") {
            lines[1]
        } else {
            continue;
        };
        let text_start = if lines[0].contains("-->") { 1 } else { 2 };
        let text = lines[text_start..].join("\n");
        if text.trim().is_empty() {
            continue;
        }
        let timing = time_line.replace(',', ".");
        out.push_str(&timing);
        out.push('\n');
        out.push_str(&text);
        out.push_str("\n\n");
    }

    if out.trim() == "WEBVTT" {
        return Err("no valid SRT cues".into());
    }
    Ok(out)
}

/// SAMI/SMI → WebVTT. `</SYNC>` 없이 다음 `<SYNC>` 로만 구분하는 한국 SMI 도 지원.
fn smi_to_vtt(content: &str) -> Result<String, String> {
    let lower = content.to_lowercase();
    let body_end = lower
        .find("</body>")
        .unwrap_or(content.len());

    let mut syncs: Vec<(usize, u64)> = Vec::new();
    let mut pos = 0usize;
    while pos < body_end {
        let Some(rel) = lower[pos..body_end].find("<sync") else {
            break;
        };
        let abs = pos + rel;
        let after_tag = &content[abs..];
        let Some(gt) = after_tag.find('>') else {
            pos = abs + 5;
            continue;
        };
        let tag = &after_tag[..gt + 1];
        let Some(start_ms) = parse_smi_start(tag) else {
            pos = abs + 5;
            continue;
        };
        let body_start = abs + gt + 1;
        syncs.push((body_start, start_ms));
        pos = body_start;
    }

    if syncs.is_empty() {
        return Err("no SMI cues found".into());
    }

    let mut out = String::from("WEBVTT\n\n");
    for (i, (body_start, start_ms)) in syncs.iter().enumerate() {
        let body_limit = syncs
            .get(i + 1)
            .map(|(next, _)| next.saturating_sub(1))
            .unwrap_or(body_end);
        let raw = &content[*body_start..body_limit.min(content.len())];
        let text = smi_body_to_text(raw);
        if text.is_empty() {
            continue;
        }
        let end_ms = syncs
            .get(i + 1)
            .map(|(_, ms)| *ms)
            .unwrap_or(start_ms + 3000);
        out.push_str(&format!(
            "{} --> {}\n{}\n\n",
            ms_to_vtt(*start_ms),
            ms_to_vtt(end_ms),
            text
        ));
    }

    if out.trim() == "WEBVTT" {
        return Err("no SMI cues found".into());
    }
    Ok(out)
}

fn parse_smi_start(tag: &str) -> Option<u64> {
    let lower = tag.to_lowercase();
    let idx = lower.find("start")?;
    let mut rest = tag[idx + 5..].trim_start();
    rest = rest.strip_prefix('=')?;
    let digits: String = rest
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn smi_body_to_text(raw: &str) -> String {
    let mut text = raw.to_string();
    for (from, to) in [
        ("<br>", "\n"),
        ("<BR>", "\n"),
        ("<br/>", "\n"),
        ("<BR/>", "\n"),
        ("<p>", ""),
        ("</p>", "\n"),
        ("<P>", ""),
        ("</P>", "\n"),
        ("&nbsp;", " "),
        ("&#160;", " "),
    ] {
        text = text.replace(from, to);
    }
    strip_html_tags(&text)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && *l != "\u{00a0}")
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_html_tags(input: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

fn ass_to_vtt(content: &str) -> Result<String, String> {
    let mut out = String::from("WEBVTT\n\n");
    let mut count = 0usize;
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("Dialogue:") {
            continue;
        }
        let parts: Vec<&str> = trimmed.splitn(10, ',').collect();
        if parts.len() < 10 {
            continue;
        }
        let start = ass_time_to_vtt(parts[1].trim())?;
        let end = ass_time_to_vtt(parts[2].trim())?;
        let text = strip_ass_tags(parts[9]);
        if text.is_empty() {
            continue;
        }
        out.push_str(&format!("{start} --> {end}\n{text}\n\n"));
        count += 1;
    }
    if count == 0 {
        return Err("no ASS/SSA dialogue lines found".into());
    }
    Ok(out)
}

fn ass_time_to_vtt(t: &str) -> Result<String, String> {
    // H:MM:SS.cc
    let pieces: Vec<&str> = t.split(':').collect();
    if pieces.len() != 3 {
        return Err(format!("invalid ASS time: {t}"));
    }
    let hours: u64 = pieces[0].parse().map_err(|_| format!("invalid ASS time: {t}"))?;
    let minutes: u64 = pieces[1].parse().map_err(|_| format!("invalid ASS time: {t}"))?;
    let sec_parts: Vec<&str> = pieces[2].split('.').collect();
    let seconds: u64 = sec_parts[0]
        .parse()
        .map_err(|_| format!("invalid ASS time: {t}"))?;
    let centis: u64 = sec_parts
        .get(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let total_ms = hours * 3_600_000 + minutes * 60_000 + seconds * 1_000 + centis * 10;
    Ok(ms_to_vtt(total_ms))
}

fn strip_ass_tags(text: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '{' => in_tag = true,
            '}' => in_tag = false,
            '\\' if !in_tag => {}
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("\\N", "\n")
        .replace("\\n", "\n")
        .trim()
        .to_string()
}

fn ms_to_vtt(ms: u64) -> String {
    let d = Duration::from_millis(ms);
    let h = d.as_secs() / 3600;
    let m = (d.as_secs() % 3600) / 60;
    let s = d.as_secs() % 60;
    let millis = d.subsec_millis();
    format!("{h:02}:{m:02}:{s:02}.{millis:03}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_subtitle_names_case_insensitive() {
        assert!(matches_video_subtitle("movie.mp4", "movie.smi"));
        assert!(matches_video_subtitle("Movie.mp4", "movie.smi"));
        assert!(matches_video_subtitle("movie.mp4", "movie.ko.srt"));
        assert!(!matches_video_subtitle("movie.mp4", "other.smi"));
    }

    #[test]
    fn smi_converts_with_closing_sync() {
        let smi = r#"<SAMI><BODY><SYNC Start=1000><P>Hello</P></SYNC><SYNC Start=3000><P>World</P></SYNC></BODY></SAMI>"#;
        let vtt = smi_to_vtt(smi).unwrap();
        assert!(vtt.starts_with("WEBVTT"));
        assert!(vtt.contains("Hello"));
        assert!(vtt.contains("World"));
    }

    #[test]
    fn smi_converts_without_closing_sync() {
        let smi = r#"<SAMI><BODY><SYNC Start=1000><P>Hello</P><SYNC Start=3000><P>World</P></BODY></SAMI>"#;
        let vtt = smi_to_vtt(smi).unwrap();
        assert!(vtt.contains("Hello"));
        assert!(vtt.contains("World"));
    }

    #[test]
    fn smi_converts_euc_kr_bytes() {
        let smi = "<SYNC Start=1000><P>안녕</P><SYNC Start=3000><P>세계</P>";
        let bytes = encoding_rs::EUC_KR.encode(smi).0.into_owned();
        let decoded = decode_subtitle_bytes(&bytes).unwrap();
        let vtt = smi_to_vtt(&decoded).unwrap();
        assert!(vtt.contains("안녕"));
        assert!(vtt.contains("세계"));
    }
}
