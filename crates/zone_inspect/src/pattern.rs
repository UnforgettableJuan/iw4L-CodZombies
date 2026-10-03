pub fn is_pattern(text: &str) -> bool {
    text.contains('*')
}

pub fn matches(pattern: &str, text: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase();
    let text = text.to_ascii_lowercase();
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or("");
    let Some(mut rest) = text.strip_prefix(first) else {
        return false;
    };
    let parts: Vec<&str> = parts.collect();
    let Some((last, middle)) = parts.split_last() else {
        return rest.is_empty();
    };
    for part in middle {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    rest.len() >= last.len() && rest.ends_with(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_needs_exact_match() {
        assert!(matches("zombie_theater", "ZOMBIE_THEATER"));
        assert!(!matches("zombie_theater", "zombie_theater_patch"));
    }

    #[test]
    fn star_matches_any_run() {
        assert!(matches("zombie_*", "zombie_theater"));
        assert!(matches("zombie_*", "zombie_"));
        assert!(matches("*zombie*", "en_zombie_moon"));
        assert!(matches("*_patch", "zombie_moon_patch"));
        assert!(matches("z*e*r", "zombie_theater"));
        assert!(matches("*", ""));
        assert!(!matches("zombie_*", "mp_nuked"));
        assert!(!matches("*_patch", "patch"));
        assert!(!matches("a*a", "a"));
    }
}
