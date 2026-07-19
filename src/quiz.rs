use reqwest::blocking::Client;
use scraper::{Html, Selector};

use crate::html2text::html_to_term;
use crate::types::MOODLE_BASE;
use crate::ui::prompt_input;

fn display_quiz_questions(doc: &Html) {
    let question_sel = Selector::parse(".que").expect("bad selector: .que");
    let qtext_sel = Selector::parse(".qtext").expect("bad selector: .qtext");

    for (q_idx, question) in doc.select(&question_sel).enumerate() {
        let qtext = question
            .select(&qtext_sel)
            .next()
            .map(|q| {
                q.text()
                    .collect::<Vec<_>>()
                    .join("")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();

        println!("\n========================================");
        println!("Q{}. {}", q_idx + 1, qtext);
        println!("========================================");

        for (a_idx, row) in question
            .select(&Selector::parse(".r0, .r1").expect("bad selector: .r0"))
            .enumerate()
        {
            let label = row
                .select(&Selector::parse("label").expect("bad selector: label"))
                .next();
            let text = label
                .map(|l| {
                    l.text()
                        .collect::<Vec<_>>()
                        .join("")
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_else(|| format!("Choice {}", a_idx + 1));
            let input_name = row
                .select(
                    &Selector::parse("input[type=checkbox], input[type=radio]")
                        .expect("bad selector: checkbox"),
                )
                .next()
                .and_then(|i| i.value().attr("name"))
                .unwrap_or("");
            println!("  {}. {} [{}]", a_idx + 1, text, input_name);
        }
    }
}

pub fn handle_quiz(client: &Client, quiz_id: u32, quiz_name: &str) {
    println!("\n=== Quiz: {} ===", quiz_name);

    let quiz_url = format!("{}/mod/quiz/view.php?id={}", MOODLE_BASE, quiz_id);
    let form_sel = Selector::parse("form").expect("bad selector: form_quiz");

    let view_page = client
        .get(&quiz_url)
        .send()
        .expect("handler HTTP request failed");
    let final_url = view_page.url().to_string();
    let view_body = view_page.text().expect("read handler response failed");
    let doc_quiz = Html::parse_document(&view_body);

    // Check if already on a review page (URL contains /review.php or /summary.php)
    let is_review_page = final_url.contains("review.php") || final_url.contains("summary.php");

    if is_review_page {
        let doc = &doc_quiz;
        println!("\nReview page detected.");
        if doc
            .select(&Selector::parse(".que").expect("bad selector: que"))
            .next()
            .is_some()
        {
            display_quiz_questions(doc);
        } else {
            let text = html_to_term(&view_body);
            println!("{}", text);
        }
        return;
    }

    // Try to start a new attempt
    let cmid_input: Option<u32> = doc_quiz
        .select(&Selector::parse("input[name=cmid]").expect("bad selector: cmid"))
        .next()
        .and_then(|i| i.value().attr("value")?.parse().ok());
    let view_sesskey: Option<&str> = doc_quiz
        .select(&Selector::parse("input[name=sesskey]").expect("bad selector: sesskey"))
        .next()
        .and_then(|i| i.value().attr("value"));

    let (cmid, view_sesskey) = match (cmid_input, view_sesskey) {
        (Some(c), Some(s)) => (c, s),
        _ => {
            println!("Could not get quiz info. Displaying page content...");
            let text = html_to_term(&view_body);
            println!("{}", text);
            return;
        }
    };

    let start_resp = client
        .post(format!("{}/mod/quiz/startattempt.php", MOODLE_BASE))
        .form(&[
            ("cmid", cmid.to_string().as_str()),
            ("sesskey", view_sesskey),
        ])
        .send()
        .expect("handler HTTP request failed");
    let start_final = start_resp.url().to_string();
    let start_body = start_resp.text().expect("read handler response failed");
    let mut attempt_body = start_body;
    let mut doc_attempt = Html::parse_document(&attempt_body);

    // Check if startattempt redirected to review (already completed)
    if start_final.contains("review.php") || start_final.contains("summary.php") {
        println!("\nQuiz already completed. Showing review...");
        display_quiz_questions(&doc_attempt);
        return;
    }

    // Handle preflight confirmation form (e.g. timed quizzes)
    if doc_attempt
        .select(&form_sel)
        .any(|f| f.value().attr("id").unwrap_or("") == "mod_quiz_preflight_form")
    {
        println!("  Quiz requires confirmation (time limit, etc.). Submitting preflight form...");
        let preflight = doc_attempt
            .select(&form_sel)
            .find(|f| f.value().attr("id").unwrap_or("") == "mod_quiz_preflight_form")
            .unwrap();

        let mut preflight_params: Vec<(&str, &str)> = Vec::new();
        for inp in preflight
            .select(&Selector::parse("input[type=hidden]").expect("bad selector: preflight-hidden"))
        {
            let name = inp.value().attr("name").unwrap_or("");
            let value = inp.value().attr("value").unwrap_or("");
            if !name.is_empty() {
                preflight_params.push((name, value));
            }
        }
        preflight_params.push(("submitbutton", "受験を開始する"));

        let default_action = format!("{}/mod/quiz/startattempt.php", MOODLE_BASE);
        let preflight_action = preflight.value().attr("action").unwrap_or(&default_action);

        let attempt_resp = client
            .post(preflight_action)
            .form(&preflight_params)
            .send()
            .expect("handler HTTP request failed");
        attempt_body = attempt_resp.text().expect("read handler response failed");
        doc_attempt = Html::parse_document(&attempt_body);
    }

    let mut current_body = attempt_body;
    let mut current_doc = doc_attempt;

    loop {
        let has_questions = current_doc
            .select(&Selector::parse(".que").expect("bad selector: que"))
            .next()
            .is_some();

        let process_form = current_doc.select(&form_sel).find(|f| {
            f.value()
                .attr("action")
                .unwrap_or("")
                .contains("processattempt")
        });

        let process_form = match process_form {
            Some(f) => f,
            None => {
                if !has_questions {
                    let text = html_to_term(&current_body);
                    println!("{}", text);
                } else {
                    println!("No submission form available.");
                }
                return;
            }
        };

        if has_questions {
            println!("\nParsing quiz questions...");
            display_quiz_questions(&current_doc);

            let mut hidden_params: Vec<(&str, &str)> = Vec::new();
            let mut checkbox_names: Vec<String> = Vec::new();

            for inp in process_form.select(&Selector::parse("input").expect("bad selector: input"))
            {
                let name = inp.value().attr("name").unwrap_or("");
                let itype = inp.value().attr("type").unwrap_or("text");
                let value = inp.value().attr("value").unwrap_or("");

                if (itype == "checkbox" || itype == "radio") && name.contains("choice") {
                    if value == "1" {
                        checkbox_names.push(name.to_string());
                    }
                } else if itype == "hidden" && !name.is_empty() {
                    hidden_params.push((name, value));
                }
            }

            let unlabeled_input = process_form
                .select(&Selector::parse("input[name='']").expect("bad selector: unlabeled"))
                .next();
            let unlabeled_value = unlabeled_input.and_then(|i| {
                let v = i.value().attr("value").unwrap_or("");
                if v.is_empty() { None } else { Some(v) }
            });

            println!(
                "\nEnter choice numbers to select (comma-separated, e.g. 1,3,5), or 0 for none:"
            );
            let selection = prompt_input("Selection: ");

            let mut selected_indices: Vec<usize> = Vec::new();
            if selection != "0" && !selection.is_empty() {
                for part in selection.split(',') {
                    if let Ok(idx) = part.trim().parse::<usize>()
                        && idx > 0
                        && idx <= checkbox_names.len()
                    {
                        selected_indices.push(idx - 1);
                    }
                }
            }

            let mut params: Vec<(&str, &str)> = hidden_params.clone();
            let still_more = current_doc
                .select(
                    &Selector::parse("input[name=nextpage][value]")
                        .expect("bad selector: nextpage"),
                )
                .next()
                .and_then(|i| i.value().attr("value"))
                .map(|v| v != "-1" && !v.is_empty())
                .unwrap_or(false);
            if still_more {
                params.push(("next", "次のページ"));
            } else {
                params.push(("next", "テストを終了する ..."));
            }

            if let Some(val) = unlabeled_value {
                params.push(("_qf__mod_quiz_attempt_form", val));
            }

            for &idx in &selected_indices {
                let name = &checkbox_names[idx];
                params.push((name, "1"));
                println!("  Selected: {}", name);
            }

            let form_url = process_form.value().attr("action").unwrap().to_string();
            let resp = client
                .post(&form_url)
                .form(&params)
                .send()
                .expect("handler HTTP request failed");
            current_body = resp.text().expect("read handler response failed");
            current_doc = Html::parse_document(&current_body);

            // Check if we're on the summary page (no .que elements)
            let still_has_questions = current_doc
                .select(&Selector::parse(".que").expect("bad selector: que"))
                .next()
                .is_some();

            if !still_has_questions {
                // On the summary page, submit to finish
                let summary_form = current_doc.select(&form_sel).find(|f| {
                    f.value()
                        .attr("action")
                        .unwrap_or("")
                        .contains("processattempt")
                });

                if let Some(summary_form) = summary_form {
                    let mut final_params: Vec<(&str, &str)> = Vec::new();
                    for inp in summary_form
                        .select(&Selector::parse("input").expect("bad selector: input2"))
                    {
                        let name = inp.value().attr("name").unwrap_or("");
                        let itype = inp.value().attr("type").unwrap_or("text");
                        let value = inp.value().attr("value").unwrap_or("");

                        if itype == "checkbox" || itype == "radio" {
                            if value == "1" {
                                final_params.push((name, "1"));
                            }
                        } else if itype == "hidden" && !name.is_empty() && name != "slots" {
                            final_params.push((name, value));
                        }
                    }

                    let final_url = summary_form.value().attr("action").unwrap().to_string();
                    let resp_final = client
                        .post(&final_url)
                        .form(&final_params)
                        .send()
                        .expect("handler HTTP request failed");
                    let doc_final = Html::parse_document(
                        &resp_final.text().expect("read handler response failed"),
                    );

                    let title = doc_final
                        .select(&Selector::parse("title").expect("bad selector: title"))
                        .next()
                        .map(|t| t.text().collect::<String>())
                        .unwrap_or_default();
                    println!("\nResult: {}", title);
                    if title.contains("レビュー") || title.contains("review") {
                        println!("Quiz submitted successfully!");
                    }
                }
                return;
            }
            // Still has questions → loop to next page
        } else {
            // No questions on this page - this is unexpected after starting an attempt
            let text = html_to_term(&current_body);
            println!("{}", text);
            return;
        }
    }
}
