use std::time::Duration;

use regex::Regex;
use reqwest::blocking::Client;
use scraper::{Html, Selector};

use crate::types::MOODLE_BASE;

pub fn handle_assignment(
    client: &Client,
    assignment_id: u32,
    assignment_name: &str,
    file_path: &str,
) {
    println!("\n=== Assignment: {} ===", assignment_name);

    let view_url = format!("{}/mod/assign/view.php?id={}", MOODLE_BASE, assignment_id);
    let view_body = client
        .get(&view_url)
        .send()
        .expect("handler HTTP request failed")
        .text()
        .expect("read handler response failed");
    let view_doc = Html::parse_document(&view_body);

    let submission_status = view_doc
        .select(&Selector::parse(".submithelp").expect("bad selector: .submithelp_assign"))
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "Unknown".to_string());
    println!("Submission status: {}", submission_status);

    let edit_url = format!(
        "{}/mod/assign/view.php?id={}&action=editsubmission",
        MOODLE_BASE, assignment_id
    );
    let edit_body = client
        .get(&edit_url)
        .send()
        .expect("handler HTTP request failed")
        .text()
        .expect("read handler response failed");
    let edit_doc = Html::parse_document(&edit_body);

    let form_sel = Selector::parse("form").expect("bad selector: form_assign");
    let form = match edit_doc.select(&form_sel).next() {
        Some(f) => f,
        None => {
            eprintln!("  [!] Could not find submission form on edit page");
            return;
        }
    };

    let mut params: Vec<(String, String)> = Vec::new();
    for inp in form.select(&Selector::parse("input[type=hidden]").expect("bad selector: hidden")) {
        let name = inp.value().attr("name").unwrap_or("");
        let value = inp.value().attr("value").unwrap_or("");
        params.push((name.to_string(), value.to_string()));
    }

    let file_draft_id = params
        .iter()
        .find(|(n, _)| n == "files_filemanager")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    let sesskey = params
        .iter()
        .find(|(n, _)| n == "sesskey")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    let lastmodified = params
        .iter()
        .find(|(n, _)| n == "lastmodified")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    let userid = params
        .iter()
        .find(|(n, _)| n == "userid")
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| "12501".to_string());

    println!("  sesskey: {}", sesskey);
    println!("  file_draft_id: {}", file_draft_id);
    println!("  lastmodified: {}", lastmodified);
    println!("  userid: {}", userid);

    let author_name = {
        let usertext_sel = Selector::parse(".usertext").expect("bad selector: .usertext");
        let profile_sel =
            Selector::parse("a[href*='user/view.php']").expect("bad selector: profile");
        view_doc
            .select(&usertext_sel)
            .next()
            .or_else(|| edit_doc.select(&usertext_sel).next())
            .or_else(|| view_doc.select(&profile_sel).next())
            .or_else(|| edit_doc.select(&profile_sel).next())
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "山田 太郎".to_string())
    };
    println!("  author: {}", author_name);

    let accepted_types = Regex::new(r#""accepted_types":\s*\[([^\]]+)\]"#)
        .expect("bad regex: accepted_types")
        .captures(&edit_body)
        .and_then(|c| c.get(1))
        .map(|m| {
            let raw = m.as_str();
            let types: Vec<&str> = raw
                .split(',')
                .map(|s| s.trim().trim_matches('"').trim())
                .filter(|s| !s.is_empty())
                .collect();
            if types.is_empty() {
                ".pdf".to_string()
            } else {
                types.join(",")
            }
        })
        .unwrap_or_else(|| ".pdf".to_string());
    println!("  accepted file types: {}", accepted_types);

    println!("\nRemoving existing submission...");
    let delete_confirm_url = format!(
        "{}/mod/assign/view.php?id={}&action=removesubmissionconfirm",
        MOODLE_BASE, assignment_id
    );
    let delete_confirm_resp = client
        .get(&delete_confirm_url)
        .send()
        .expect("handler HTTP request failed");
    let delete_confirm_text = delete_confirm_resp.text().unwrap_or_default();

    let confirm_sesskey = Regex::new(r#"name="sesskey" value="([^"]+)""#)
        .expect("bad regex: sesskey")
        .captures(&delete_confirm_text)
        .map(|m| m.get(1).unwrap().as_str().to_string())
        .unwrap_or_else(|| sesskey.clone());

    let delete_params = [
        ("id", assignment_id.to_string()),
        ("action", "removesubmission".to_string()),
        ("userid", userid.clone()),
        ("sesskey", confirm_sesskey),
    ];
    client
        .post(format!("{}/mod/assign/view.php", MOODLE_BASE))
        .form(&delete_params)
        .send()
        .expect("handler HTTP request failed");

    println!("\nWaiting for draft area to be cleared...");
    let max_attempts = 5;
    let mut cleared = false;
    let final_draft_id = file_draft_id.clone();
    let final_sesskey = sesskey.clone();

    let filename_re = Regex::new(r#""filename":"([^"]+)""#).expect("bad regex: filename");
    let filecount_re = Regex::new(r#"filecount":(\d+)"#).expect("bad regex: filecount");

    for attempt in 1..=max_attempts {
        println!("  Attempt {} of {}...", attempt, max_attempts);
        std::thread::sleep(Duration::from_secs(1));

        let list_url = format!("{}/repository/draftfiles_ajax.php?action=list", MOODLE_BASE);
        let list_params = [
            ("sesskey", final_sesskey.as_str()),
            ("client_id", ""),
            ("filepath", "/"),
            ("itemid", final_draft_id.as_str()),
        ];
        let list_resp = client
            .post(&list_url)
            .form(&list_params)
            .send()
            .expect("handler HTTP request failed");
        let list_text = list_resp.text().unwrap_or_default();

        let filecount = filecount_re
            .captures(&list_text)
            .map(|m| m.get(1).unwrap().as_str().to_string())
            .unwrap_or_else(|| "0".to_string());

        println!("  Server reports filecount: {}", filecount);

        if filecount == "0" {
            println!("  Draft area is empty!");
            cleared = true;
            break;
        }

        println!(
            "  Draft area still has {} files, deleting them...",
            filecount
        );
        let filenames: Vec<String> = filename_re
            .captures_iter(&list_text)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect();

        for fname in &filenames {
            println!("    Deleting: {}", fname);
            let selected_json = format!(r#"[{{"filepath":"/","filename":"{}"}}]"#, fname);
            let del_params = [
                ("sesskey", final_sesskey.as_str()),
                ("client_id", ""),
                ("filepath", "/"),
                ("itemid", final_draft_id.as_str()),
                ("selected", selected_json.as_str()),
            ];
            let del_url = format!(
                "{}/repository/draftfiles_ajax.php?action=deleteselected",
                MOODLE_BASE
            );
            let _ = client.post(&del_url).form(&del_params).send();
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    if !cleared {
        println!(
            "  WARNING: Could not clear draft area after {} attempts, proceeding anyway...",
            max_attempts
        );
    }

    let edit_resp_final = client
        .get(&edit_url)
        .send()
        .expect("handler HTTP request failed");
    let edit_text_final = edit_resp_final.text().unwrap_or_default();

    fn extract_str(text: &str, key: &str, end_chars: &[char]) -> String {
        if let Some(pos) = text.find(key) {
            let start = pos + key.len();
            let end = start + text[start..].find(end_chars).unwrap_or(100);
            text[start..end.min(start + 100)].to_string()
        } else {
            String::new()
        }
    }

    let client_id = extract_str(
        &edit_text_final,
        "filemanager-",
        &[' ', '\n', '"', '\'', '>'],
    )
    .trim()
    .to_string();

    fn extract_number(text: &str, key: &str) -> String {
        if let Some(pos) = text.find(key) {
            let after = &text[pos + key.len()..];
            after.chars().take_while(|c| c.is_ascii_digit()).collect()
        } else {
            String::new()
        }
    }

    let ctx_id = extract_number(&edit_text_final, "ctx_id=");
    let course = extract_number(&edit_text_final, "course=");

    let maxbytes = Regex::new(r#""maxbytes":\s*"(\d+)""#)
        .expect("bad regex: maxbytes1")
        .captures(&edit_text_final)
        .or_else(|| {
            Regex::new(r#""maxbytes":\s*(\d+)"#)
                .expect("bad regex: maxbytes2")
                .captures(&edit_text_final)
        })
        .or_else(|| {
            Regex::new(r#"maxbytes["\s:=]+(\d+)"#)
                .expect("bad regex: maxbytes3")
                .captures(&edit_text_final)
        })
        .map(|m| m.get(1).unwrap().as_str().to_string())
        .unwrap_or_else(|| "5242880".to_string());

    let areamaxbytes = Regex::new(r#""areamaxbytes":\s*"(-?\d+)""#)
        .expect("bad regex: areamaxbytes1")
        .captures(&edit_text_final)
        .or_else(|| {
            Regex::new(r#""areamaxbytes":\s*(-?\d+)"#)
                .expect("bad regex: areamaxbytes2")
                .captures(&edit_text_final)
        })
        .map(|m| m.get(1).unwrap().as_str().to_string())
        .unwrap_or_else(|| "-1".to_string());

    let repo_id = Regex::new(r#""repo_id":\s*"(\d+)""#)
        .expect("bad regex: repo_id1")
        .captures(&edit_text_final)
        .or_else(|| {
            Regex::new(r#""repo_id":\s*(\d+)"#)
                .expect("bad regex: repo_id2")
                .captures(&edit_text_final)
        })
        .or_else(|| {
            Regex::new(r#"data-repositoryid="(\d+)""#)
                .expect("bad regex: repo_id3")
                .captures(&edit_text_final)
        })
        .map(|m| m.get(1).unwrap().as_str().to_string())
        .unwrap_or_else(|| "4".to_string());

    let env = "filemanager".to_string();
    let savepath = "/".to_string();

    println!("\n  ctx_id: {}", ctx_id);
    println!("  course: {}", course);
    println!("  client_id: {}", client_id);
    println!("  maxbytes: {}", maxbytes);
    println!("  areamaxbytes: {}", areamaxbytes);
    println!("  repo_id: {}", repo_id);

    let upload_url = format!(
        "{}/repository/repository_ajax.php?action=upload&sesskey={}&ctx_id={}&course={}&itemid={}&client_id={}&repo_id={}&env={}&maxbytes={}&areamaxbytes={}&p={}&page=",
        MOODLE_BASE,
        final_sesskey,
        ctx_id,
        course,
        final_draft_id,
        client_id,
        repo_id,
        env,
        maxbytes,
        areamaxbytes,
        savepath
    );

    let file_name = std::path::Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file.pdf")
        .to_string();

    let upload_form = reqwest::blocking::multipart::Form::new()
        .text("itemid", final_draft_id.clone())
        .text("accepted_types[]", accepted_types.clone())
        .text("title", file_name.clone())
        .text("author", author_name.clone())
        .file("repo_upload_file", file_path)
        .unwrap();

    let upload_resp = match client.post(&upload_url).multipart(upload_form).send() {
        Ok(r) => r,
        Err(e) => {
            println!("\nUpload request FAILED: {}", e);
            return;
        }
    };

    let upload_status = upload_resp.status();
    let upload_text = upload_resp.text().unwrap_or_default();

    println!("  Upload status: {}", upload_status);
    println!("  Upload body: {}", upload_text);

    if upload_text.contains("\"error\"") {
        println!("\n[!] Upload returned error!");
        return;
    } else if upload_text.contains("\"url\"")
        && upload_text.contains("\"file\"")
        && !upload_text.contains("\"event\":\"fileexists\"")
    {
        println!("\n[+] Upload succeeded!");
    } else if upload_text.contains("\"event\":\"fileexists\"") {
        println!("\n[!] File already exists - attempting overwrite...");

        let existingfile_re =
            Regex::new(r#""existingfile":\{"filepath":"([^"]+)","filename":"([^"]+)"#)
                .expect("bad regex: existingfile");
        let newfile_re = Regex::new(r#""newfile":\{"filepath":"([^"]+)","filename":"([^"]+)"#)
            .expect("bad regex: newfile");
        let existingfile_caps = existingfile_re.captures(&upload_text);
        let newfile_caps = newfile_re.captures(&upload_text);

        if let Some(existing_m) = existingfile_caps {
            let existingfilepath = existing_m
                .get(1)
                .map(|m| m.as_str().replace("\\/", "/"))
                .unwrap_or("/".to_string());
            let existingfilename = existing_m
                .get(2)
                .map(|m| m.as_str())
                .unwrap_or("")
                .to_string();
            let newfilename = newfile_caps
                .as_ref()
                .and_then(|m| m.get(2).map(|nm| nm.as_str().to_string()))
                .unwrap_or_default();

            println!("  Overwriting: {}", existingfilename);

            let overwrite_url = format!(
                "{}/repository/draftfiles_ajax.php?action=overwrite",
                MOODLE_BASE
            );
            let overwrite_params = [
                ("sesskey", final_sesskey.as_str()),
                ("client_id", client_id.as_str()),
                ("filepath", "/"),
                ("itemid", final_draft_id.as_str()),
                ("existingfilename", existingfilename.as_str()),
                ("existingfilepath", existingfilepath.as_str()),
                ("newfilename", existingfilename.as_str()),
                ("newfilepath", existingfilepath.as_str()),
            ];

            let overwrite_resp = client
                .post(&overwrite_url)
                .form(&overwrite_params)
                .send()
                .expect("handler HTTP request failed");
            let overwrite_text = overwrite_resp.text().unwrap_or_default();
            println!("  Overwrite response: {}", overwrite_text);

            if overwrite_text.contains("\"event\":\"overwritten\"")
                || overwrite_text.contains("\"filepath\"")
                || overwrite_text == "true"
                || overwrite_text.is_empty()
            {
                println!("\n[+] File overwritten successfully!");
            } else if overwrite_text.contains("\"error\"") || overwrite_text == "false" {
                println!("\n[!] Overwrite failed, trying delete-then-upload...");
                let delete_url = format!(
                    "{}/repository/draftfiles_ajax.php?action=deleteselected",
                    MOODLE_BASE
                );
                let file_to_delete = if !existingfilename.is_empty() {
                    existingfilename.clone()
                } else {
                    newfilename.clone()
                };

                let selected_json =
                    format!(r#"[{{"filepath":"/","filename":"{}"}}]"#, file_to_delete);
                let del_params = [
                    ("sesskey", final_sesskey.as_str()),
                    ("client_id", client_id.as_str()),
                    ("filepath", "/"),
                    ("itemid", final_draft_id.as_str()),
                    ("selected", selected_json.as_str()),
                ];
                client
                    .post(&delete_url)
                    .form(&del_params)
                    .send()
                    .expect("handler HTTP request failed");
                std::thread::sleep(Duration::from_secs(2));

                let list_url =
                    format!("{}/repository/draftfiles_ajax.php?action=list", MOODLE_BASE);
                let list_params = [
                    ("sesskey", final_sesskey.as_str()),
                    ("client_id", client_id.as_str()),
                    ("filepath", "/"),
                    ("itemid", final_draft_id.as_str()),
                ];
                let list_resp = client
                    .post(&list_url)
                    .form(&list_params)
                    .send()
                    .expect("handler HTTP request failed");
                let list_text = list_resp.text().unwrap_or_default();
                println!("  List after delete: {}", list_text);

                let retry_upload_url = format!(
                    "{}/repository/repository_ajax.php?action=upload&sesskey={}&ctx_id={}&course={}&itemid={}&client_id={}&repo_id={}&env={}&maxbytes={}&areamaxbytes={}&p={}&page=",
                    MOODLE_BASE,
                    final_sesskey,
                    ctx_id,
                    course,
                    final_draft_id,
                    client_id,
                    repo_id,
                    env,
                    maxbytes,
                    areamaxbytes,
                    savepath
                );

                let file_name = std::path::Path::new(file_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file.pdf")
                    .to_string();

                let retry_upload_form = reqwest::blocking::multipart::Form::new()
                    .text("itemid", final_draft_id.clone())
                    .text("accepted_types[]", accepted_types.clone())
                    .text("title", file_name.clone())
                    .text("author", author_name.clone())
                    .file("repo_upload_file", file_path)
                    .unwrap();

                let retry_resp = client
                    .post(&retry_upload_url)
                    .multipart(retry_upload_form)
                    .send()
                    .expect("handler HTTP request failed");
                let retry_text = retry_resp.text().unwrap_or_default();
                println!("  Retry upload response: {}", retry_text);

                if !retry_text.contains("\"error\"")
                    && (retry_text.contains("\"filepath\"")
                        || retry_text.contains("\"file\"")
                        || retry_text == "true")
                {
                    println!("\n[+] Upload succeeded after delete!");
                } else {
                    println!("\n[!] Retry upload failed!");
                    return;
                }
            } else {
                println!("\n[?] Overwrite response unclear: {}", overwrite_text);
                return;
            }
        } else {
            println!("\n[!] Could not parse fileexists response");
            return;
        }
    } else {
        println!("\n[+] Upload succeeded!");
    }

    println!("\nRe-fetching edit page after upload...");
    let edit_after_upload_resp = client
        .get(&edit_url)
        .send()
        .expect("handler HTTP request failed");
    let edit_after_upload_text = edit_after_upload_resp.text().unwrap_or_default();

    let mut fresh_lastmodified = lastmodified.clone();
    let mut final_sesskey_for_submit = sesskey.clone();

    if let Some(lm_match) = Regex::new(r#"name="lastmodified" value="(\d+)""#)
        .expect("bad regex: lastmodified")
        .captures(&edit_after_upload_text)
    {
        fresh_lastmodified = lm_match
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or(lastmodified.clone());
    }
    if let Some(sk_match) = Regex::new(r#"name="sesskey" value="([^"]+)""#)
        .expect("bad regex: sesskey_capture")
        .captures(&edit_after_upload_text)
    {
        final_sesskey_for_submit = sk_match
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or(sesskey.clone());
    }

    println!("  fresh lastmodified: {}", fresh_lastmodified);
    println!("  fresh sesskey: {}", final_sesskey_for_submit);

    println!("\nSubmitting assignment...");
    let submit_url = format!("{}/mod/assign/view.php", MOODLE_BASE);

    let submit_form = reqwest::blocking::multipart::Form::new()
        .text("action", "savesubmission")
        .text("id", assignment_id.to_string())
        .text("userid", userid.clone())
        .text("lastmodified", fresh_lastmodified)
        .text("sesskey", final_sesskey_for_submit)
        .text("_qf__mod_assign_submission_form", "1")
        .text("files_filemanager", final_draft_id.clone())
        .text("submitbutton", "提出する".to_string());

    let submit_resp = match client.post(&submit_url).multipart(submit_form).send() {
        Ok(r) => r,
        Err(e) => {
            println!("\nSubmit request FAILED: {}", e);
            return;
        }
    };

    let submit_status = submit_resp.status();
    let submit_body = submit_resp.text().unwrap_or_default();
    println!("  Submit status: {}", submit_status);
    println!("  Body length: {} bytes", submit_body.len());

    if submit_body.contains("submissionstatussubmitted") {
        println!("\n[+] ASSIGNMENT SUBMISSION SUCCEEDED!");
    } else if submit_body.contains("success") || submit_body.contains("Submitted") {
        println!("\n[+] Submission may have succeeded!");
    } else if submit_body.contains("\"error\"") || submit_body.contains("エラー") {
        println!("\n[!] Submission may have failed.");
    } else {
        println!("\n[?] Submission status unclear");
    }
}
