use regex::Regex;

pub fn strip_tag_regions(text: &str, open_tag: &str, close_tag: &str) -> String {
    let mut result = String::new();
    let mut i = 0;
    while i < text.len() {
        if text[i..]
            .to_lowercase()
            .starts_with(&open_tag.to_lowercase())
            && let Some(end) = text[i..].to_lowercase().find(&close_tag.to_lowercase())
        {
            i += end + close_tag.len();
            continue;
        }
        result.push(text[i..].chars().next().unwrap());
        i += text[i..].chars().next().unwrap().len_utf8();
    }
    result
}

pub fn html_to_term(html: &str) -> String {
    let mut s = strip_tag_regions(html, "<script", "</script>");
    s = strip_tag_regions(&s, "<style", "</style>");
    s = s
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n");

    let mut pre_contents: Vec<String> = Vec::new();

    let pre_pat = Regex::new(r"(?is)<pre[^>]*>.*?</pre>").unwrap();
    let code_pat = Regex::new(r"(?is)<code[^>]*>.*?</code>").unwrap();
    let all_tag = Regex::new(r"<[^>]+>").unwrap();

    let decode_entities = |t: String| -> String {
        t.replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&nbsp;", " ")
            .replace("&mdash;", "—")
            .replace("&#x3a;", ":")
            .replace("&#x2f;", "/")
    };

    let mut i = 0usize;
    s = pre_pat
        .replace_all(&s, |caps: &regex::Captures| {
            let raw = caps.get(0).map_or("", |m| m.as_str());
            let inner = all_tag.replace_all(raw, "");
            let inner = decode_entities(inner.to_string());
            pre_contents.push(inner);
            let p = format!("\n```PRECONT{}```\n", i);
            i += 1;
            p
        })
        .to_string();

    s = code_pat
        .replace_all(&s, |caps: &regex::Captures| {
            let raw = caps.get(0).map_or("", |m| m.as_str());
            let inner = all_tag.replace_all(raw, "");
            let inner = decode_entities(inner.to_string());
            pre_contents.push(inner);
            let p = format!("\n```PRECONT{}```\n", i);
            i += 1;
            p
        })
        .to_string();

    s = Regex::new(r"(?i)</p>")
        .unwrap()
        .replace_all(&s, "\n\n")
        .to_string();
    s = Regex::new(r"(?i)<h1[^>]*>")
        .unwrap()
        .replace_all(&s, "\n# ")
        .to_string();
    s = Regex::new(r"(?i)<h2[^>]*>")
        .unwrap()
        .replace_all(&s, "\n## ")
        .to_string();
    s = Regex::new(r"(?i)<h3[^>]*>")
        .unwrap()
        .replace_all(&s, "\n### ")
        .to_string();
    s = Regex::new(r"(?i)</h[1-6]>")
        .unwrap()
        .replace_all(&s, "\n")
        .to_string();
    s = Regex::new(r"(?i)<li[^>]*>")
        .unwrap()
        .replace_all(&s, "\n- ")
        .to_string();
    s = Regex::new(r"(?i)</li>")
        .unwrap()
        .replace_all(&s, "")
        .to_string();
    s = Regex::new(r"(?i)<(?:ul|ol)[^>]*>")
        .unwrap()
        .replace_all(&s, "\n")
        .to_string();
    s = Regex::new(r"(?i)</(?:ul|ol)>")
        .unwrap()
        .replace_all(&s, "\n")
        .to_string();
    s = Regex::new(r"(?i)<strong[^>]*>")
        .unwrap()
        .replace_all(&s, "**")
        .to_string();
    s = Regex::new(r"(?i)</strong>")
        .unwrap()
        .replace_all(&s, "**")
        .to_string();
    s = Regex::new(r"(?i)<b[^>]*>")
        .unwrap()
        .replace_all(&s, "**")
        .to_string();
    s = Regex::new(r"(?i)</b>")
        .unwrap()
        .replace_all(&s, "**")
        .to_string();
    s = Regex::new(r"(?i)<em[^>]*>")
        .unwrap()
        .replace_all(&s, "*")
        .to_string();
    s = Regex::new(r"(?i)</em>")
        .unwrap()
        .replace_all(&s, "*")
        .to_string();
    s = Regex::new(r"(?i)<i[^>]*>")
        .unwrap()
        .replace_all(&s, "*")
        .to_string();
    s = Regex::new(r"(?i)</i>")
        .unwrap()
        .replace_all(&s, "*")
        .to_string();
    s = Regex::new(r#"(?i)<a\s+[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#)
        .unwrap()
        .replace_all(&s, "[$2]($1)")
        .to_string();
    s = Regex::new(r#"(?i)<img\s+[^>]*src="([^"]+)"[^>]*(?:alt="([^"]*)")?[^>]*>"#)
        .unwrap()
        .replace_all(&s, "\n![$2]($1)\n")
        .to_string();
    s = Regex::new(r"<[^>]+>")
        .unwrap()
        .replace_all(&s, "")
        .to_string();
    s = s
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x3a;", ":")
        .replace("&#x2f;", "/")
        .replace("&nbsp;", " ")
        .replace("&mdash;", "—");

    for (idx, content) in pre_contents.drain(..).enumerate() {
        let ph = format!("```PRECONT{}```", idx);
        s = s.replace(&ph, &format!("```\n{}\n```", content));
    }

    s = Regex::new(r"\n{2,}- ")
        .unwrap()
        .replace_all(&s, "\n- ")
        .to_string();
    s = Regex::new(r"\n- \s*\n")
        .unwrap()
        .replace_all(&s, "\n- ")
        .to_string();
    s = Regex::new(r"- [ \t]+")
        .unwrap()
        .replace_all(&s, "- ")
        .to_string();
    s = Regex::new(r"(?m)^ +")
        .unwrap()
        .replace_all(&s, "")
        .to_string();
    s = Regex::new(r"\n{3,}")
        .unwrap()
        .replace_all(&s, "\n\n")
        .to_string();
    s = Regex::new(r"[ \t]+\n")
        .unwrap()
        .replace_all(&s, "\n")
        .to_string();
    s.trim().to_string()
}
