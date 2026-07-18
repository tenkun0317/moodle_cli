use std::env;
use std::io::Write;
use std::time::Duration;

use regex::Regex;
use reqwest::blocking::Client;

mod api;
mod assignment;
mod auth;
mod html2text;
mod quiz;
mod resource;
mod types;
mod ui;

use api::{get_course_sections, get_courses, get_section_quizzes};
use assignment::handle_assignment;
use auth::login;
use quiz::handle_quiz;
use resource::handle_url;
use types::MOODLE_BASE;
use ui::{print_menu, prompt_input};

fn main() {
    println!("=== Moodle CLI ===\n");

    let args: Vec<String> = env::args().collect();

    let (username, password, direct_assign_id, direct_file_path) = if args.len() >= 5 {
        (
            args[1].clone(),
            args[2].clone(),
            Some(args[3].clone()),
            Some(args[4].clone()),
        )
    } else if args.len() >= 3 {
        (args[1].clone(), args[2].clone(), None, None)
    } else if args.len() == 2 && (args[1] == "-h" || args[1] == "--help") {
        println!("Usage:");
        println!(
            "  {} <username> <password>                   Interactive mode",
            args[0]
        );
        println!(
            "  {} <username> <password> <assign_id> <file>  Direct submission",
            args[0]
        );
        return;
    } else if args.len() > 1 {
        eprintln!(
            "Usage: {} <username> <password> [assign_id] [file]",
            args[0]
        );
        return;
    } else {
        let u = prompt_input("Username: ");
        let p = {
            print!("Password: ");
            std::io::stdout().flush().unwrap();
            rpassword::read_password().unwrap()
        };
        (u, p, None, None)
    };

    let client = Client::builder()
        .cookie_store(true)
        .timeout(Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36")
        .build()
        .expect("HTTP client build failed");

    println!("Logging in...");
    if !login(&client, &username, &password) {
        eprintln!("Login failed!");
        return;
    }
    println!("Logged in successfully!\n");

    if let (Some(assign_id), Some(file_path)) = (direct_assign_id, direct_file_path) {
        let id: u32 = match assign_id.parse() {
            Ok(n) => n,
            Err(_) => {
                eprintln!("Invalid assignment ID: {}", assign_id);
                return;
            }
        };
        let name = format!("Assignment {}", id);
        handle_assignment(&client, id, &name, &file_path);
        println!("\nDone.");
        return;
    }

    let accepted_types_re =
        Regex::new(r#""accepted_types":\s*\[([^\]]+)\]"#).expect("bad regex: accepted_types");

    loop {
        let courses = get_courses(&client);
        if courses.is_empty() {
            println!("No courses found.");
            break;
        }

        let course_names: Vec<&str> = courses.iter().map(|c| c.name.as_str()).collect();
        let course_choice = match print_menu("Select Course", &course_names, false) {
            Some(n) => n,
            None => break,
        };
        let course = &courses[course_choice - 1];
        println!("\n>>> Selected: {} (id={})", course.name, course.id);

        let sections = get_course_sections(&client, course.id);
        if sections.is_empty() {
            println!("No sections found.");
            continue;
        }

        let section_names: Vec<&str> = sections.iter().map(|s| s.name.as_str()).collect();
        let section_choice = match print_menu(
            &format!("Sections in {}", course.name),
            &section_names,
            true,
        ) {
            Some(0) => continue,
            Some(n) => n - 1,
            None => break,
        };
        let section = &sections[section_choice];
        println!("\n>>> Selected: {} (section={})", section.name, section.id);

        let activities = get_section_quizzes(&client, course.id, section.id);
        if activities.is_empty() {
            println!("No quizzes or assignments found in this course.");
            continue;
        }

        let activity_names: Vec<&str> = activities.iter().map(|q| q.name.as_str()).collect();
        let activity_choice = match print_menu(
            &format!("Activities in {} - {}", course.name, section.name),
            &activity_names,
            true,
        ) {
            Some(0) => continue,
            Some(n) => n - 1,
            None => break,
        };
        let activity = &activities[activity_choice];
        println!("\n>>> Selected: {} (id={})", activity.name, activity.id);

        if activity.is_assignment() {
            let edit_url = format!(
                "{}/mod/assign/view.php?id={}&action=editsubmission",
                MOODLE_BASE, activity.id
            );
            if let Ok(edit_body) = client
                .get(&edit_url)
                .send()
                .map(|r| r.text().unwrap_or_default())
            {
                let types = accepted_types_re
                    .captures(&edit_body)
                    .and_then(|c| c.get(1))
                    .map(|m| {
                        let raw = m.as_str();
                        let t: Vec<&str> = raw
                            .split(',')
                            .map(|s| s.trim().trim_matches('"').trim())
                            .filter(|s| !s.is_empty())
                            .collect();
                        if t.is_empty() {
                            ".pdf".to_string()
                        } else {
                            t.join(", ")
                        }
                    })
                    .unwrap_or_else(|| ".pdf".to_string());
                println!("  Accepted file types: {}", types);
            }
            let file_path = prompt_input("Enter file path: ");
            if file_path.is_empty() {
                println!("No file provided, skipping.");
                continue;
            }
            if !std::path::Path::new(&file_path).exists() {
                println!("File not found: {}", file_path);
                continue;
            }
            handle_assignment(&client, activity.id, &activity.name, &file_path);
        } else if activity.is_url() {
            handle_url(&client, activity.id, &activity.name);
        } else {
            handle_quiz(&client, activity.id, &activity.name);
        }
    }

    println!("\nGoodbye!");
}
