use std::fs;
use std::path::Path;
use std::time::Duration;

use reqwest::blocking::Client;
use scraper::{ElementRef, Html, Selector};

use crate::html2text::{html_to_term, strip_tag_regions};
use crate::types::{DEBUG_DIR, MOODLE_BASE};

fn ensure_debug_dir() {
    fs::create_dir_all(DEBUG_DIR).ok();
}

fn download_file(client: &Client, url_name: &str, final_url: &str) {
    let safe_name: String = url_name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == ' ')
        .collect();
    let dl_dir = Path::new("downloads").join(safe_name.trim());
    let _ = fs::create_dir_all(&dl_dir);
    let fname = final_url.rsplit('/').next().unwrap_or("file");
    let fname = fname.split('?').next().unwrap_or(fname);
    let dest = dl_dir.join(fname);
    if dest.exists() && dest.metadata().map(|m| m.len()).unwrap_or(0) > 0 {
        let size_kb = dest
            .metadata()
            .map(|m| m.len() as f64 / 1024.0)
            .unwrap_or(0.0);
        println!(
            "  Already downloaded: {} ({:.1} KB)",
            dest.display(),
            size_kb
        );
        return;
    }
    if let Ok(resp) = client.get(final_url).send()
        && let Ok(bytes) = resp.bytes()
    {
        let _ = fs::write(&dest, &bytes);
        let size_kb = bytes.len() as f64 / 1024.0;
        println!("  Downloaded: {} ({:.1} KB)", dest.display(), size_kb);
    }
}

fn download_images(client: &Client, url_name: &str, content_html: &str, final_url: &str) {
    let img_sel = Selector::parse("img").expect("bad selector: img");
    let img_doc = Html::parse_fragment(content_html);
    let images: Vec<ElementRef> = img_doc.select(&img_sel).collect();
    if images.is_empty() {
        return;
    }
    println!("\n--- Images ---");
    let safe_name: String = url_name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == ' ')
        .collect();
    let dl_dir = Path::new("downloads").join(safe_name.trim());
    fs::create_dir_all(&dl_dir).ok();
    let mut downloaded = 0u32;
    for img in &images {
        let src = img.value().attr("src").unwrap_or("").to_string();
        let alt = img.value().attr("alt").unwrap_or("");
        let base = match url::Url::parse(final_url) {
            Ok(u) => u,
            Err(e) => {
                eprintln!("  [!] Invalid base URL for image resolution: {e}");
                continue;
            }
        };
        let full = base
            .join(&src)
            .map(|u| u.to_string())
            .unwrap_or_else(|_| src.clone());
        println!(
            "  ![{}]({})",
            if alt.is_empty() { "image" } else { alt },
            full
        );
        let fname = full.rsplit('/').next().unwrap_or("image.png");
        let fname = fname.split('?').next().unwrap_or(fname);
        let dest = dl_dir.join(fname);
        let valid_exists = dest.exists() && {
            let data = fs::read(&dest).unwrap_or_default();
            data.len() >= 8
                && matches!(&data[..4], b"\x89PNG" | b"\xFF\xD8\xFF" | b"GIF8" | b"RIFF")
        };
        if !valid_exists
            && let Ok(img_bytes) = client.get(&full).send()
            && let Ok(bytes) = img_bytes.bytes()
        {
            let valid = bytes.len() >= 8
                && matches!(
                    &bytes[..4],
                    b"\x89PNG" | b"\xFF\xD8\xFF" | b"GIF8" | b"RIFF"
                );
            if valid {
                let _ = fs::write(&dest, &bytes);
                downloaded += 1;
            }
        }
    }
    if downloaded > 0 {
        println!("  (saved {} images to {})", downloaded, dl_dir.display());
    }
}

pub fn handle_url(client: &Client, url_id: u32, url_name: &str) {
    println!("\n=== URL: {} ===", url_name);

    let initial_url = format!("{}/mod/url/view.php?id={}", MOODLE_BASE, url_id);

    let response = (|| -> Option<reqwest::blocking::Response> {
        for attempt in 1..=3 {
            match client.get(&initial_url).send() {
                Ok(resp) => return Some(resp),
                Err(e) => {
                    if attempt < 3 {
                        println!("  (retry {} after send error: {})", attempt, e);
                        std::thread::sleep(Duration::from_millis(1000 * attempt));
                    } else {
                        eprintln!("  Error after 3 retries: {}", e);
                    }
                }
            }
        }
        None
    })();

    let response = match response {
        Some(r) => r,
        None => return,
    };

    let final_url = response.url().to_string();
    if final_url != initial_url {
        println!("  (redirected to: {})", final_url);
    }

    let final_url_lower = final_url.to_lowercase();
    let extension = final_url_lower.rsplit('.').next().unwrap_or("");
    let is_file = matches!(
        extension,
        "pdf" | "zip" | "mp4" | "png" | "jpg" | "jpeg" | "gif"
    );
    if is_file {
        download_file(client, url_name, &final_url);
        return;
    }

    let body = response.text().unwrap_or_default();
    if body.is_empty() {
        return;
    }

    ensure_debug_dir();
    let _ = fs::write(
        Path::new(DEBUG_DIR).join(format!("url_{}.html", url_id)),
        &body,
    );
    let doc = Html::parse_document(&body);

    let content_selectors = [
        ".resourcecontent",
        ".generalbox",
        "#region-main",
        "[role='main']",
        ".no-overflow",
        ".card-body",
        "#page-content",
        ".activity-header",
    ];
    let mut content_html = String::new();
    for sel in &content_selectors {
        if let Some(el) = doc
            .select(&Selector::parse(sel).expect("bad selector: content"))
            .next()
        {
            let html = el.inner_html();
            if !html.trim().is_empty() {
                content_html = html;
                println!("  (content from: {})", sel);
                break;
            }
        }
    }

    if content_html.is_empty()
        && let Some(body) = doc
            .select(&Selector::parse("body").expect("bad selector: body"))
            .next()
    {
        let html = body.inner_html();
        content_html = strip_tag_regions(&html, "<style", "</style>");
        println!("  (content from: <body>)");
    }
    if content_html.trim().is_empty() {
        println!("  (no content found)");
        return;
    }

    let text = html_to_term(&content_html);
    println!("{}", text);

    download_images(client, url_name, &content_html, &final_url);
}
