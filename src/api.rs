use reqwest::blocking::Client;
use scraper::{Html, Selector};

use crate::types::{ActivityType, Course, MOODLE_BASE, QuizAssignment, Section};

pub fn extract_id_from_href(href: &str, prefix: &str) -> Option<u32> {
    let query_start = href.find('?')?;
    let query = &href[query_start + 1..];
    for part in query.split('&') {
        if let Some(stripped) = part.strip_prefix(prefix) {
            return stripped.parse().ok();
        }
    }
    None
}

pub fn get_courses(client: &Client) -> Vec<Course> {
    let mut courses = Vec::new();
    let urls = [
        format!("{}/my/index.php", MOODLE_BASE),
        format!("{}/", MOODLE_BASE),
    ];
    for url in &urls {
        let body = client
            .get(url)
            .send()
            .expect("API HTTP request failed")
            .text()
            .expect("read API response failed");
        let doc = Html::parse_document(&body);
        for link in doc.select(&Selector::parse("a").expect("bad selector: a")) {
            let href = link.value().attr("href").unwrap_or("");
            let text = link.text().collect::<String>().trim().to_string();
            if href.contains("course/view.php?id=")
                && !text.is_empty()
                && let Some(id) = extract_id_from_href(href, "id=")
                && !courses.iter().any(|c: &Course| c.id == id)
            {
                courses.push(Course { id, name: text });
            }
        }
        if !courses.is_empty() {
            break;
        }
    }
    courses
}

pub fn get_course_sections(client: &Client, course_id: u32) -> Vec<Section> {
    let mut sections = Vec::new();
    let url = format!("{}/course/view.php?id={}", MOODLE_BASE, course_id);
    let body = client
        .get(&url)
        .send()
        .expect("API HTTP request failed")
        .text()
        .expect("read API response failed");
    let doc = Html::parse_document(&body);
    for section in doc.select(&Selector::parse(".section").expect("bad selector: .section")) {
        let id = section.value().attr("id").unwrap_or("");
        let id_num = id.replace("section-", "").parse().ok().unwrap_or(0);
        let name = section
            .select(&Selector::parse("h3, .section-title, .sectionname").expect("bad selector: h3"))
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| format!("Section {}", id_num));
        if id_num > 0 {
            sections.push(Section { id: id_num, name });
        }
    }
    sections
}

pub fn get_section_quizzes(
    client: &Client,
    course_id: u32,
    section_id: u32,
) -> Vec<QuizAssignment> {
    let mut quizzes = Vec::new();
    let url = format!(
        "{}/course/view.php?id={}&section={}",
        MOODLE_BASE, course_id, section_id
    );
    let body = client
        .get(&url)
        .send()
        .expect("API HTTP request failed")
        .text()
        .expect("read API response failed");
    let doc = Html::parse_document(&body);
    for link in doc.select(&Selector::parse("a").unwrap()) {
        let href = link.value().attr("href").unwrap_or("");
        let text = link.text().collect::<String>().trim().to_string();
        let activity_type = if href.contains("mod/quiz/view.php?id=") {
            Some(ActivityType::Quiz)
        } else if href.contains("mod/assign/view.php?id=") {
            Some(ActivityType::Assignment)
        } else if href.contains("mod/url/view.php?id=") {
            Some(ActivityType::Url)
        } else {
            None
        };
        if let Some(at) = activity_type
            && let Some(id) = extract_id_from_href(href, "id=")
            && !quizzes.iter().any(|q: &QuizAssignment| q.id == id)
        {
            quizzes.push(QuizAssignment {
                id,
                name: text,
                activity_type: at,
            });
        }
    }
    quizzes
}
