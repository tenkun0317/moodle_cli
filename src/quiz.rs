use reqwest::blocking::Client;
use scraper::{Html, Selector};

use crate::types::MOODLE_BASE;
use crate::ui::prompt_input;

fn display_quiz_questions(doc: &Html) {
    let question_sel = Selector::parse(".que").expect("bad selector: .que");
    let qtext_sel = Selector::parse(".qtext").expect("bad selector: .qtext");
    let answer_row_sel = Selector::parse(".r0, .r1").expect("bad selector: .r0");
    let answer_text_sel = Selector::parse(".d-flex p").expect("bad selector: .d-flex p");

    for (q_idx, question) in doc.select(&question_sel).enumerate() {
        let qtext_html = question
            .select(&qtext_sel)
            .next()
            .map(|q| q.html())
            .unwrap_or_default();

        let clean = qtext_html
            .replace("<br>", "\n")
            .replace("<br/>", "\n")
            .replace("<br />", "\n")
            .replace("</p>", "\n")
            .replace("<p>", "")
            .replace("</div>", "\n")
            .replace("<div>", "");

        let mut lines: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut in_tag = false;
        for ch in clean.chars() {
            if ch == '<' {
                in_tag = true;
                if !current.trim().is_empty() {
                    lines.push(current.trim().to_string());
                    current.clear();
                }
            } else if ch == '>' {
                in_tag = false;
            } else if !in_tag {
                current.push(ch);
            }
        }
        if !current.trim().is_empty() {
            lines.push(current.trim().to_string());
        }

        println!("\n========================================");
        println!("Q{}.", q_idx + 1);
        for line in lines {
            if !line.is_empty() {
                println!("{}", line);
            }
        }
        println!("========================================");

        let answer_rows: Vec<_> = question.select(&answer_row_sel).collect();
        for (a_idx, row) in answer_rows.iter().enumerate() {
            let choice_text = row
                .select(&answer_text_sel)
                .next()
                .map(|p| p.text().collect::<String>().trim().to_string())
                .unwrap_or_else(|| format!("Choice {}", a_idx + 1));
            let input_el = row
                .select(&Selector::parse("input[type=checkbox]").expect("bad selector: checkbox"))
                .next();
            let input_name = input_el.and_then(|i| i.value().attr("name")).unwrap_or("");
            println!("  {}. {} [{}]", a_idx + 1, choice_text, input_name);
        }
    }
}

pub fn handle_quiz(client: &Client, quiz_id: u32, quiz_name: &str) {
    println!("\n=== Quiz: {} ===", quiz_name);

    let quiz_url = format!("{}/mod/quiz/view.php?id={}", MOODLE_BASE, quiz_id);
    let form_sel = Selector::parse("form").expect("bad selector: form_quiz");

    let quiz = client
        .get(&quiz_url)
        .send()
        .expect("handler HTTP request failed");
    let doc_quiz = Html::parse_document(&quiz.text().expect("read handler response failed"));

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
            println!("Could not get quiz info.");
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
    let doc_start = Html::parse_document(&start_resp.text().expect("read handler response failed"));

    println!("\nParsing quiz questions...");
    display_quiz_questions(&doc_start);

    let process_form = match doc_start.select(&form_sel).find(|f| {
        f.value()
            .attr("action")
            .unwrap_or("")
            .contains("processattempt")
    }) {
        Some(f) => f,
        None => {
            println!("\nCould not find quiz form (may already be completed or not started).");
            return;
        }
    };

    let mut hidden_params: Vec<(&str, &str)> = Vec::new();
    let mut checkbox_names: Vec<String> = Vec::new();

    for inp in process_form.select(&Selector::parse("input").expect("bad selector: input")) {
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

    println!("\nEnter choice numbers to select (comma-separated, e.g. 1,3,5), or 0 for none:");
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
    params.push(("next", "テストを終了する ..."));

    if let Some(val) = unlabeled_value {
        params.push(("_qf__mod_quiz_attempt_form", val));
    }

    for &idx in &selected_indices {
        let name = &checkbox_names[idx];
        params.push((name, "1"));
        println!("  Selected: {}", name);
    }

    let form_url = process_form.value().attr("action").unwrap().to_string();
    let resp7 = client
        .post(&form_url)
        .form(&params)
        .send()
        .expect("handler HTTP request failed");
    let doc7 = Html::parse_document(&resp7.text().expect("read handler response failed"));

    if let Some(summary_form) = doc7.select(&form_sel).find(|f| {
        f.value()
            .attr("action")
            .unwrap_or("")
            .contains("processattempt")
    }) {
        let mut final_params: Vec<(&str, &str)> = Vec::new();
        for inp in summary_form.select(&Selector::parse("input").expect("bad selector: input2")) {
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
        let resp8 = client
            .post(&final_url)
            .form(&final_params)
            .send()
            .expect("handler HTTP request failed");
        let doc8 = Html::parse_document(&resp8.text().expect("read handler response failed"));

        let title = doc8
            .select(&Selector::parse("title").expect("bad selector: title8"))
            .next()
            .map(|t| t.text().collect::<String>())
            .unwrap_or_default();
        println!("\nResult: {}", title);

        if title.contains("レビュー") || title.contains("review") {
            println!("Quiz submitted successfully!");
        }
    } else if let Some(title) = doc7
        .select(&Selector::parse("title").expect("bad selector: title7"))
        .next()
    {
        let title = title.text().collect::<String>();
        println!("\nResult: {}", title);
        if title.contains("レビュー") || title.contains("review") {
            println!("Quiz submitted successfully!");
        }
    }
}
