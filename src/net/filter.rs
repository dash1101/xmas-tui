//! What others write, made fit to show: links hidden (each one kept, for a
//! button to show it when wanted) and, unless that's turned off, strong
//! language starred out.

/// Words starred out whole (only as a word of their own: "ass", not
/// "class"); and the starts of words starred out whatever follows them
/// ("fuck…", "shit…").
const WHOLE: &[&str] = &[
    "ass", "arse", "asses", "damn", "dammit", "goddamn", "hell", "crap", "piss", "pissed", "cock", "cocks", "dick", "dicks", "prick", "pussy", "twat", "tits", "titty", "slut", "sluts", "whore", "whores", "wanker", "bollocks", "bugger", "jackass", "dumbass", "fag", "fags", "homo", "retard", "retards", "retarded", "spic", "kike", "chink", "gook", "wetback", "tranny", "skank",
];
const START: &[&str] = &["fuck", "fuk", "fck", "shit", "sh1t", "bitch", "bastard", "cunt", "asshole", "arsehole", "motherf", "faggot", "nigg", "dickhead", "douche", "cocksuck", "jerkoff", "wank", "bullshit"];

/// Letters as they're often swapped in (1 for i, 0 for o, $ for s…).
fn plain(c: char) -> char {
    match c {
        '0' => 'o',
        '1' | '!' | '|' => 'i',
        '3' => 'e',
        '4' | '@' => 'a',
        '5' | '$' => 's',
        '7' => 't',
        c => c.to_ascii_lowercase(),
    }
}

fn rude(word: &str) -> bool {
    let w: String = word.chars().map(plain).filter(|c| c.is_ascii_alphabetic()).collect();
    // (letters held down: "fuuuck")
    let mut squeezed = String::new();
    for c in w.chars() {
        if !squeezed.ends_with(c) {
            squeezed.push(c);
        }
    }
    let starts = |s: &str| START.iter().any(|p| s.starts_with(p));
    WHOLE.contains(&w.as_str()) || WHOLE.contains(&squeezed.as_str()) || starts(&w) || starts(&squeezed)
}

/// Strong language starred out (each word's first letter kept).
pub fn words(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if !word.is_empty() {
            if rude(word) {
                let mut cs = word.chars();
                if let Some(f) = cs.next() {
                    out.push(f);
                }
                out.extend(cs.map(|_| '*'));
            } else {
                out.push_str(word);
            }
            word.clear();
        }
    };
    for c in text.chars() {
        if c.is_alphanumeric() || "@$!|".contains(c) {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

/// Endings that make a word a web address (a link without "http").
const TLDS: &[&str] = &["com", "net", "org", "io", "gg", "xyz", "app", "dev", "co", "me", "ly", "tv", "ru", "cn", "uk", "de", "info", "biz", "online", "site", "link", "click", "top", "shop", "live", "fun", "us", "ca", "au", "be", "to", "sh", "so", "ai", "cc", "ws", "tk", "ml", "ga", "cf", "gq"];

fn linky(word: &str) -> bool {
    let w = word.trim_matches(|c: char| ".,!?;:()[]{}<>\"'".contains(c)).to_ascii_lowercase();
    if w.contains("://") || w.starts_with("www.") || w.starts_with("discord.gg") {
        return true;
    }
    // something.tld (or something.tld/…)
    let host = w.split('/').next().unwrap_or("");
    let parts: Vec<&str> = host.split('.').collect();
    parts.len() >= 2 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')) && TLDS.contains(parts.last().unwrap_or(&""))
}

/// Links taken out (shown as "🔗link"), and the links themselves.
/// These words as well (a public village's own list, from its relay):
/// starred out like the rest, whatever their case.
pub fn also(text: &str, extra: &[String]) -> String {
    if extra.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if extra.iter().any(|w| w.eq_ignore_ascii_case(word)) {
            let mut cs = word.chars();
            if let Some(f) = cs.next() {
                out.push(f);
            }
            out.extend(cs.map(|_| '*'));
        } else {
            out.push_str(word);
        }
        word.clear();
    };
    for c in text.chars() {
        if c.is_alphanumeric() || c == '\'' {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

pub fn links(text: &str) -> (String, Vec<String>) {
    let mut found = vec![];
    let shown: Vec<String> = text
        .split(' ')
        .map(|w| {
            if linky(w) {
                found.push(w.to_string());
                "🔗link".to_string()
            } else {
                w.to_string()
            }
        })
        .collect();
    (shown.join(" "), found)
}

/// Both: the text to show, and the links hidden from it.
pub fn tidy(text: &str, clean_words: bool) -> (String, Vec<String>) {
    let (t, l) = links(text);
    (if clean_words { words(&t) } else { t }, l)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strong_language_is_starred_but_not_innocent_words() {
        assert_eq!(words("what the fuck is this shit"), "what the f*** is this s***");
        assert_eq!(words("F U C K? no: fuuuuck, sh1t, $hit"), "F U C K? no: f******, s***, $***");
        assert_eq!(words("a class act, passing the assessment, hello"), "a class act, passing the assessment, hello");
        assert_eq!(words("you ass"), "you a**");
        assert_eq!(words("Scunthorpe is fine; cocktail too"), "Scunthorpe is fine; cocktail too");
    }

    #[test]
    fn links_are_hidden_and_kept() {
        let (t, l) = links("come see https://evil.example/x and www.site.org or free-robux.gg, not 1.5 or end.");
        assert_eq!(t, "come see 🔗link and 🔗link or 🔗link not 1.5 or end.");
        assert_eq!(l, vec!["https://evil.example/x", "www.site.org", "free-robux.gg,"]);
        let (t, l) = links("no links here, honest");
        assert_eq!((t.as_str(), l.len()), ("no links here, honest", 0));
    }
}
