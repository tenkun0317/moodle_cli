use std::io::{self, Write};

pub fn prompt_input(prompt: &str) -> String {
    print!("{}", prompt);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

pub fn print_menu(title: &str, items: &[&str], show_back: bool) -> Option<usize> {
    loop {
        println!("\n=== {} ===", title);
        for (i, name) in items.iter().enumerate() {
            println!("  {}. {}", i + 1, name);
        }
        if show_back {
            println!("  0. Back");
        }
        println!("  q. Quit");

        let choice_str = prompt_input("Enter choice: ");
        if choice_str == "q" || choice_str == "Q" {
            return None;
        }

        let choice: usize = match choice_str.parse() {
            Ok(n) if n <= items.len() => n,
            _ => {
                println!("Invalid choice.");
                continue;
            }
        };

        if choice == 0 && show_back {
            return Some(0);
        }

        return Some(choice);
    }
}
