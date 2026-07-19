use std::env;
use std::fs;
use std::path::Path;

use reqwest::blocking::Client;
use scraper::{ElementRef, Html, Selector};

use crate::types::{DEBUG_DIR, LOGIN_URL, MOODLE_BASE};
use crate::ui::{prompt_input, prompt_yes_no};

fn save_debug_page(name: &str, body: &str) {
    let dir = Path::new(DEBUG_DIR);
    fs::create_dir_all(dir).ok();
    let path = dir.join(format!("auth_{name}.html"));
    let _ = fs::write(&path, body);
}

fn get_totp_secret() -> Option<String> {
    env::var("UEC_KEY").ok()
}

fn generate_totp(secret_str: &str) -> String {
    let secret = totp_rs::Secret::Encoded(secret_str.to_string())
        .to_bytes()
        .expect("TOTP secret decode failed");
    let totp = totp_rs::TOTP::new(
        totp_rs::Algorithm::SHA1,
        6,
        1,
        30,
        secret,
        None,
        "uec".to_string(),
    )
    .expect("TOTP initialization failed");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time went backwards")
        .as_secs();
    totp.generate(now)
}

fn process_saml_response(client: &Client, saml_form: ElementRef) -> bool {
    let saml_action = saml_form
        .value()
        .attr("action")
        .expect("SAML form missing action")
        .to_string();
    let relay_sel = Selector::parse("input[name=RelayState]").expect("bad selector for RelayState");
    let saml_sel =
        Selector::parse("input[name=SAMLResponse]").expect("bad selector for SAMLResponse");
    let relay_state = saml_form.select(&relay_sel).next().map(|i| {
        i.value()
            .attr("value")
            .expect("RelayState missing value")
            .to_string()
    });
    let saml_response = saml_form.select(&saml_sel).next().map(|i| {
        i.value()
            .attr("value")
            .expect("SAMLResponse missing value")
            .to_string()
    });

    if let (Some(relay), Some(response)) = (relay_state, saml_response) {
        let action_decoded = saml_action.replace("&#x3a;", ":").replace("&#x2f;", "/");
        match client
            .post(&action_decoded)
            .form(&[("RelayState", &relay), ("SAMLResponse", &response)])
            .send()
        {
            Ok(_) => true,
            Err(e) => {
                eprintln!("  [!] SAML POST failed: {e}");
                false
            }
        }
    } else {
        false
    }
}

pub fn login(client: &Client, username: &str, password: &str) -> bool {
    let form_sel = Selector::parse("form").expect("bad selector: form");
    let csrf_sel = Selector::parse("input[name=csrf_token]").expect("bad selector: csrf_token");
    let authcode_sel = Selector::parse("input[name=authcode]").expect("bad selector: authcode");
    let mailotp_sel = Selector::parse("input[name=mailotp]").expect("bad selector: mailotp");
    let title_sel = Selector::parse("title").expect("bad selector: title");

    println!("  Getting initial login page...");
    let resp = match client.get(LOGIN_URL).send() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("  [!] Failed to reach login page: {e}");
            return false;
        }
    };
    let origin = resp.url().origin().ascii_serialization();
    let body = resp.text().unwrap_or_default();
    save_debug_page("01_initial_login", &body);
    if body.is_empty() {
        eprintln!("  [!] Empty login page");
        return false;
    }
    let doc = Html::parse_document(&body);
    let first_form = match doc.select(&form_sel).next() {
        Some(f) => f,
        None => {
            eprintln!("  [!] No form on login page");
            return false;
        }
    };
    let action = first_form.value().attr("action").unwrap_or("");
    let csrf = first_form
        .select(&csrf_sel)
        .next()
        .map(|i| i.value().attr("value").unwrap_or("").to_string());

    println!("  Posting initial form...");
    let resp2 = match client
        .post(format!("{}{}", origin, action))
        .form(&[
            ("csrf_token", csrf.as_deref().unwrap_or("")),
            ("shib_idp_ls_supported", "false"),
            ("_eventId_proceed", ""),
        ])
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("  [!] Failed to post initial form: {e}");
            return false;
        }
    };
    let _shib_origin = resp2.url().origin().ascii_serialization();
    let body2 = resp2.text().unwrap_or_default();
    save_debug_page("02_idp_login", &body2);
    if body2.is_empty() {
        eprintln!("  [!] Empty response after initial form");
        return false;
    }
    let doc2 = Html::parse_document(&body2);
    let login_form = match doc2.select(&form_sel).next() {
        Some(f) => f,
        None => {
            eprintln!("  [!] No login form on shibboleth page");
            return false;
        }
    };
    let login_action = login_form.value().attr("action").unwrap_or("");
    let login_csrf = login_form
        .select(&csrf_sel)
        .next()
        .map(|i| i.value().attr("value").unwrap_or("").to_string())
        .unwrap_or_default();

    println!("  Posting credentials...");
    let resp3 = match client
        .post(format!("https://shibboleth.cc.uec.ac.jp{}", login_action))
        .form(&[
            ("j_username", username),
            ("j_password", password),
            ("csrf_token", &login_csrf),
            ("_eventId_proceed", ""),
            ("donotcache", "1"),
        ])
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("  [!] Credential submission failed: {e}");
            return false;
        }
    };
    let body3 = resp3.text().unwrap_or_default();
    save_debug_page("03_after_credentials", &body3);
    let doc3 = Html::parse_document(&body3);

    let has_totp = doc3.select(&authcode_sel).next().is_some();
    let has_mailotp = doc3.select(&mailotp_sel).next().is_some();

    if has_totp || has_mailotp {
        let use_mailotp = if has_totp && has_mailotp {
            prompt_yes_no("  Use mail OTP instead of TOTP?")
        } else {
            has_mailotp
        };

        println!("  2FA required...");

        let otp_code = if use_mailotp {
            prompt_input("  Mail OTP code: ")
        } else if let Some(secret) = get_totp_secret() {
            println!("  Auto-generating TOTP...");
            generate_totp(&secret)
        } else {
            println!("  Enter TOTP code manually:");
            prompt_input("  TOTP code: ")
        };

        let otp_form = match doc3.select(&form_sel).next() {
            Some(f) => f,
            None => {
                eprintln!("  [!] No OTP form found");
                return false;
            }
        };
        let otp_action = otp_form.value().attr("action").unwrap_or("");
        let otp_csrf = otp_form
            .select(&csrf_sel)
            .next()
            .map(|i| i.value().attr("value").unwrap_or("").to_string())
            .unwrap_or_default();

        let otp_url = format!("https://shibboleth.cc.uec.ac.jp{}", otp_action);
        let (mfa_field, mfa_value) = if use_mailotp {
            ("mailotp", "mailotp")
        } else {
            ("authcode", "totp")
        };
        let form_data: Vec<(&str, &str)> = vec![
            ("csrf_token", &otp_csrf),
            ("mfa_type", mfa_value),
            (mfa_field, &otp_code),
            ("login", "ログイン"),
        ];
        let resp_otp = match client.post(&otp_url).form(&form_data).send() {
            Ok(r) => r,
            Err(e) => {
                eprintln!("  [!] 2FA submission failed: {e}");
                return false;
            }
        };
        let body_otp = resp_otp.text().unwrap_or_default();
        save_debug_page("04_after_otp", &body_otp);
        let otp_doc = Html::parse_document(&body_otp);

        if otp_doc.select(&authcode_sel).next().is_some()
            || otp_doc.select(&mailotp_sel).next().is_some()
        {
            eprintln!("  [!] OTP code rejected (still on OTP page)");
            return false;
        }

        match otp_doc.select(&form_sel).next() {
            Some(saml_form) => {
                if !process_saml_response(client, saml_form) {
                    eprintln!("  [!] SAML response submission failed after 2FA");
                    return false;
                }
            }
            None => {
                eprintln!("  [!] No SAML form after 2FA");
                return false;
            }
        }
    } else {
        match doc3.select(&form_sel).next() {
            Some(saml_form) => {
                if !process_saml_response(client, saml_form) {
                    eprintln!("  [!] SAML response submission failed");
                    return false;
                }
            }
            None => {
                eprintln!("  [!] No SAML form after login");
                return false;
            }
        }
    }

    let test_resp = match client.get(format!("{}/my/", MOODLE_BASE)).send() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("  [!] Dashboard check failed: {e}");
            return false;
        }
    };
    let final_url = test_resp.url().as_str().to_string();
    let test_body = test_resp.text().unwrap_or_default();
    save_debug_page("05_dashboard", &test_body);
    let test_doc = Html::parse_document(&test_body);
    let test_title = test_doc
        .select(&title_sel)
        .next()
        .map(|t| t.text().collect::<String>())
        .unwrap_or_default();

    let on_moodle = final_url.starts_with(MOODLE_BASE);
    let on_login_page = test_title.contains("ログイン") || test_title.contains("Login");

    if !on_moodle || on_login_page {
        println!("  [!] Still on login page after authentication (redirected to: {final_url})");
        false
    } else {
        println!("  Login successful!");
        true
    }
}
