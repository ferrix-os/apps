//! The account and the computer's name: what is typed, what is derived
//! from it, and what is refused, in words.

/// Names the system keeps for itself.
pub const RESERVED: [&str; 10] = [
    "root", "auth", "nobody", "wheel", "daemon", "bin", "sys", "sshd", "adm", "nogroup",
];

/// The user name made from a full name: its first word, lower case, kept
/// to the letters a user name may have. `Ada Lovelace` is `ada`.
#[must_use]
pub fn derive_user(fullname: &str) -> String {
    let first = fullname.split_whitespace().next().unwrap_or("");
    let mut user: String = first
        .chars()
        .flat_map(char::to_lowercase)
        .filter_map(|c| match c {
            'a'..='z' | '0'..='9' | '_' | '-' => Some(c),
            'ä' => Some('a'),
            'ö' => Some('o'),
            'ü' => Some('u'),
            'é' | 'è' | 'ê' | 'ë' => Some('e'),
            'á' | 'à' | 'â' | 'å' => Some('a'),
            'í' | 'ì' | 'î' | 'ï' => Some('i'),
            'ó' | 'ò' | 'ô' | 'ø' => Some('o'),
            'ú' | 'ù' | 'û' => Some('u'),
            'ñ' => Some('n'),
            'ç' => Some('c'),
            'ß' => Some('s'),
            _ => None,
        })
        .collect();
    if user.starts_with(|c: char| c.is_ascii_digit() || c == '-') {
        user.insert(0, '_');
    }
    user.chars().take(32).collect()
}

/// Why `user` cannot be a user name, or `None` when it can:
/// `[a-z_][a-z0-9_-]{0,31}`, and none the system keeps.
#[must_use]
pub fn user_problem(user: &str) -> Option<String> {
    let mut chars = user.chars();
    let Some(first) = chars.next() else {
        return Some("Choose a user name.".to_owned());
    };
    if user.chars().count() > 32 {
        return Some("A user name has at most 32 characters.".to_owned());
    }
    if !(first.is_ascii_lowercase() || first == '_') {
        return Some("A user name starts with a small letter or _.".to_owned());
    }
    if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
        return Some("Use only small letters, digits, - and _.".to_owned());
    }
    if RESERVED.contains(&user) {
        return Some(format!("“{user}” is kept by the system; choose another."));
    }
    None
}

/// Why `host` cannot be the computer's name, or `None`:
/// `[a-z0-9-]{1,63}`.
#[must_use]
pub fn hostname_problem(host: &str) -> Option<String> {
    if host.is_empty() {
        return Some("Choose a name for the computer.".to_owned());
    }
    if host.len() > 63 {
        return Some("A computer's name has at most 63 characters.".to_owned());
    }
    if !host
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Some("Use only small letters, digits and -.".to_owned());
    }
    None
}

/// What is wrong with the password and its confirmation: an error that
/// stops the page, or a warning that does not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Password {
    /// It is fine.
    Fine,
    /// It may be used, but is weak.
    Weak(String),
    /// It cannot be used.
    Refused(String),
}

/// Judge `password` and `confirm`.
#[must_use]
pub fn password(password: &str, confirm: &str) -> Password {
    if password.is_empty() {
        return Password::Refused("Choose a password.".to_owned());
    }
    if password != confirm {
        return Password::Refused("The passwords do not match.".to_owned());
    }
    if password.chars().count() < 8 {
        return Password::Weak("Shorter than 8 characters: easy to guess.".to_owned());
    }
    Password::Fine
}

#[cfg(test)]
mod tests {
    use super::{Password, derive_user, hostname_problem, password, user_problem};

    #[test]
    fn a_user_name_comes_from_the_first_name() {
        assert_eq!(derive_user("Ada Lovelace"), "ada");
        assert_eq!(derive_user("  Jürgen Groß"), "jurgen");
        assert_eq!(derive_user("3jane"), "_3jane");
        assert_eq!(derive_user(""), "");
    }

    #[test]
    fn user_names_are_checked() {
        assert_eq!(user_problem("ada"), None);
        assert_eq!(user_problem("_x-1"), None);
        assert!(user_problem("").is_some());
        assert!(user_problem("Ada").is_some());
        assert!(user_problem("1ada").is_some());
        assert!(user_problem("a.b").is_some());
        assert!(user_problem("root").is_some());
        assert!(user_problem("auth").is_some());
        assert!(user_problem("nobody").is_some());
        assert!(user_problem(&"a".repeat(33)).is_some());
        assert_eq!(user_problem(&"a".repeat(32)), None);
    }

    #[test]
    fn computer_names_are_checked() {
        assert_eq!(hostname_problem("ferrix"), None);
        assert_eq!(hostname_problem("ada-laptop-2"), None);
        assert!(hostname_problem("").is_some());
        assert!(hostname_problem("Ada").is_some());
        assert!(hostname_problem("a_b").is_some());
        assert!(hostname_problem(&"a".repeat(64)).is_some());
    }

    #[test]
    fn passwords_must_match_and_are_warned_when_short() {
        assert!(matches!(password("", ""), Password::Refused(_)));
        assert!(matches!(password("abc", "abd"), Password::Refused(_)));
        assert!(matches!(password("abc", "abc"), Password::Weak(_)));
        assert_eq!(password("correct horse", "correct horse"), Password::Fine);
    }
}
