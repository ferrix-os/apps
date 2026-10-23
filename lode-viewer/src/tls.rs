//! "Forwarded over TLS": a machine set up to send its log with lode's own
//! upload to `lode-receive` on this machine.
//!
//! `lode-receive pair --machine NAME --server-name HOST --json` makes (or
//! finds) the certificates and says what the machine needs: files to copy,
//! a line for `/etc/ferrix/lode-upload.conf`, and the command that restarts
//! the upload. All of it is shown to the person first, as the commands that
//! will run; only on their confirm is it done, over ssh, as root on the
//! machine. Then `lode-receive check` says whether the receiver runs here,
//! and `lode-receive install-service` starts it as the user's own service
//! when it does not.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::remote::Remote;

/// A JSON value, as much of JSON as `lode-receive --json` writes.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number, as its text.
    Number(String),
    /// A string.
    Str(String),
    /// An array.
    Array(Vec<Value>),
    /// An object, its pairs in order.
    Object(Vec<(String, Value)>),
}

impl Value {
    /// A member of an object.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Value> {
        match self {
            Value::Object(pairs) => pairs.iter().find(|(key, _)| key == name).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The string it is.
    #[must_use]
    pub fn str(&self) -> Option<&str> {
        match self {
            Value::Str(text) => Some(text),
            _ => None,
        }
    }

    /// A member's string, or empty.
    #[must_use]
    pub fn text(&self, name: &str) -> String {
        self.get(name)
            .and_then(Value::str)
            .unwrap_or_default()
            .to_owned()
    }
}

/// Parse one JSON value.
#[must_use]
pub fn parse_json(text: &str) -> Option<Value> {
    let mut chars = text.trim().chars().peekable();
    let value = value(&mut chars)?;
    while chars.peek().is_some_and(|c| c.is_whitespace()) {
        let _ = chars.next();
    }
    chars.peek().is_none().then_some(value)
}

/// The characters left.
type Chars<'a> = std::iter::Peekable<std::str::Chars<'a>>;

fn space(chars: &mut Chars<'_>) {
    while chars.peek().is_some_and(|c| c.is_whitespace()) {
        let _ = chars.next();
    }
}

fn value(chars: &mut Chars<'_>) -> Option<Value> {
    space(chars);
    match *chars.peek()? {
        '{' => {
            let _ = chars.next();
            let mut pairs = Vec::new();
            space(chars);
            if chars.peek() == Some(&'}') {
                let _ = chars.next();
                return Some(Value::Object(pairs));
            }
            loop {
                space(chars);
                let Value::Str(key) = value(chars)? else {
                    return None;
                };
                space(chars);
                (chars.next()? == ':').then_some(())?;
                pairs.push((key, value(chars)?));
                space(chars);
                match chars.next()? {
                    ',' => {}
                    '}' => return Some(Value::Object(pairs)),
                    _ => return None,
                }
            }
        }
        '[' => {
            let _ = chars.next();
            let mut items = Vec::new();
            space(chars);
            if chars.peek() == Some(&']') {
                let _ = chars.next();
                return Some(Value::Array(items));
            }
            loop {
                items.push(value(chars)?);
                space(chars);
                match chars.next()? {
                    ',' => {}
                    ']' => return Some(Value::Array(items)),
                    _ => return None,
                }
            }
        }
        '"' => {
            let _ = chars.next();
            let mut text = String::new();
            loop {
                match chars.next()? {
                    '"' => return Some(Value::Str(text)),
                    '\\' => match chars.next()? {
                        'n' => text.push('\n'),
                        't' => text.push('\t'),
                        'r' => text.push('\r'),
                        'b' => text.push('\u{8}'),
                        'f' => text.push('\u{c}'),
                        'u' => {
                            let hex: String = (0..4).filter_map(|_| chars.next()).collect();
                            let code = u32::from_str_radix(&hex, 16).ok()?;
                            text.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        }
                        other => text.push(other),
                    },
                    other => text.push(other),
                }
            }
        }
        _ => {
            let mut word = String::new();
            while let Some(&c) = chars.peek() {
                if c == ',' || c == '}' || c == ']' || c.is_whitespace() {
                    break;
                }
                word.push(c);
                let _ = chars.next();
            }
            match word.as_str() {
                "null" => Some(Value::Null),
                "true" => Some(Value::Bool(true)),
                "false" => Some(Value::Bool(false)),
                _ if !word.is_empty() => Some(Value::Number(word)),
                _ => None,
            }
        }
    }
}

/// One file the machine needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteFile {
    /// Here.
    pub local: PathBuf,
    /// On the machine.
    pub remote: String,
    /// Its mode, octal.
    pub mode: String,
}

/// What pairing says the machine needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// The machine's name at the receiver.
    pub machine: String,
    /// Where the receiver listens.
    pub listen: String,
    /// The CA's fingerprint.
    pub ca_fingerprint: String,
    /// The server certificate's.
    pub server_fingerprint: String,
    /// Files to copy.
    pub files: Vec<RemoteFile>,
    /// The upload's configuration file on the machine.
    pub config_file: String,
    /// Its line for this receiver.
    pub config_line: String,
    /// The command that restarts the upload.
    pub restart: String,
}

/// `lode-receive pair --json`'s object as a plan.
#[must_use]
pub fn plan(value: &Value) -> Option<Plan> {
    let files = match value.get("remote_files")? {
        Value::Array(items) => items
            .iter()
            .map(|item| {
                Some(RemoteFile {
                    local: PathBuf::from(item.get("local")?.str()?),
                    remote: item.get("remote")?.str()?.to_owned(),
                    mode: match item.get("mode")? {
                        Value::Str(mode) | Value::Number(mode) => mode.clone(),
                        _ => return None,
                    },
                })
            })
            .collect::<Option<Vec<_>>>()?,
        _ => return None,
    };
    let plan = Plan {
        machine: value.text("machine"),
        listen: value.text("listen"),
        ca_fingerprint: value.text("ca_fingerprint"),
        server_fingerprint: value.text("server_fingerprint"),
        files,
        config_file: value.text("remote_config_file"),
        config_line: value.text("remote_config_line"),
        restart: value.text("remote_restart"),
    };
    let safe = |path: &str| path.starts_with('/') && !path.contains(['\'', '\n', '\0']);
    let ok = !plan.machine.is_empty()
        && plan.config_line.starts_with(&format!("{} ", plan.machine))
        && !plan.config_line.contains('\n')
        && safe(&plan.config_file)
        && plan.files.iter().all(|file| {
            safe(&file.remote)
                && file.mode.chars().all(|c| c.is_ascii_digit())
                && !file.mode.is_empty()
        })
        && !plan.restart.is_empty()
        && !plan.restart.contains(['\n', '\'']);
    ok.then_some(plan)
}

/// The configuration file with the machine's line in place of the one
/// that names it, or added.
#[must_use]
pub fn new_config(old: &str, plan: &Plan) -> String {
    let prefix = format!("{} ", plan.machine);
    let mut out = String::new();
    let mut placed = false;
    for line in old.lines() {
        if line.starts_with(&prefix) {
            if !placed {
                out.push_str(&plan.config_line);
                out.push('\n');
                placed = true;
            }
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !placed {
        out.push_str(&plan.config_line);
        out.push('\n');
    }
    out
}

/// A path in single quotes for the machine's shell.
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// The directory a path is in.
fn parent(path: &str) -> &str {
    path.rsplit_once('/')
        .map_or("/", |(dir, _)| if dir.is_empty() { "/" } else { dir })
}

/// The commands run on the machine, in order, as the person is shown them
/// before anything is done.
#[must_use]
pub fn remote_commands(plan: &Plan) -> Vec<String> {
    let mut out = Vec::new();
    for file in &plan.files {
        out.push(format!(
            "mkdir -p {} && cat > {} && chmod {} {}   < {}",
            quote(parent(&file.remote)),
            quote(&file.remote),
            file.mode,
            quote(&file.remote),
            file.local.display()
        ));
    }
    out.push(format!(
        "write {}: the line `{}` in place of any line starting `{} `",
        plan.config_file, plan.config_line, plan.machine
    ));
    out.push(plan.restart.clone());
    out
}

/// The receiver's program: `LODE_RECEIVE_BIN`, else beside the viewer,
/// else `lode-receive` on the path.
#[must_use]
pub fn receiver() -> Option<PathBuf> {
    if let Some(bin) = std::env::var_os("LODE_RECEIVE_BIN") {
        return Some(PathBuf::from(bin));
    }
    // Installed beside the viewer, as install-linux.sh puts it.
    if let Some(beside) = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join("lode-receive")))
        .filter(|bin| bin.is_file())
    {
        return Some(beside);
    }
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join("lode-receive"))
            .find(|bin| bin.is_file())
    })
}

/// A receiver command's JSON, or why it failed.
fn receive(args: &[&str], root: Option<&PathBuf>) -> Result<(i32, Value), String> {
    let bin = receiver().ok_or("lode-receive is not installed here")?;
    let mut command = Command::new(&bin);
    command.args(args).arg("--json");
    if let Some(root) = root {
        command.arg("--root").arg(root);
    }
    let output = command
        .output()
        .map_err(|error| format!("{}: {error}", bin.display()))?;
    let code = output.status.code().unwrap_or(1);
    let text = String::from_utf8_lossy(&output.stdout);
    let value = parse_json(&text).ok_or_else(|| {
        format!(
            "lode-receive {} said no JSON (exit {code}): {}",
            args.first().copied().unwrap_or(""),
            String::from_utf8_lossy(&output.stderr).trim()
        )
    })?;
    if let Some(error) = value.get("error").and_then(Value::str) {
        return Err(format!("lode-receive: {error}"));
    }
    Ok((code, value))
}

/// Pair `machine` for a receiver the machine reaches as `server`.
///
/// # Errors
///
/// The receiver missing, refusing, or saying what this cannot use.
pub fn pair(machine: &str, server: &str, root: Option<&PathBuf>) -> Result<Plan, String> {
    let (_, value) = receive(
        &["pair", "--machine", machine, "--server-name", server],
        root,
    )?;
    plan(&value).ok_or_else(|| "lode-receive pair said something this cannot use".to_owned())
}

/// Make sure the receiver runs here: check, and install its service when
/// it does not run. Answers what it found.
///
/// # Errors
///
/// The receiver missing or failing.
pub fn ensure_running(root: Option<&PathBuf>) -> Result<String, String> {
    match receive(&["check"], root) {
        Ok((0, value)) => Ok(format!(
            "lode-receive runs, listening on {}",
            value.text("listen")
        )),
        Ok((3, _)) | Err(_) => {
            let (_, value) = receive(&["install-service"], root)?;
            Ok(format!(
                "lode-receive installed as {}, started: {}",
                value.text("unit"),
                value.get("started") == Some(&Value::Bool(true))
            ))
        }
        Ok((code, _)) => Err(format!("lode-receive check: exit {code}")),
    }
}

/// Run `script` on the machine through ssh, `input` on its standard input.
fn on_machine(remote: &Remote, script: &str, input: &[u8]) -> Result<Vec<u8>, String> {
    let mut command = remote.ssh_command(script);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("ssh: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input);
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("ssh: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "`{script}` on {}: {}",
            remote.host,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

/// Do `plan` on the machine, a step at a time, saying each as it is done.
///
/// # Errors
///
/// The first step that failed, with what the machine said.
pub fn apply(remote: &Remote, plan: &Plan, said: &dyn Fn(String)) -> Result<(), String> {
    for file in &plan.files {
        let bytes = std::fs::read(&file.local)
            .map_err(|error| format!("{}: {error}", file.local.display()))?;
        let script = format!(
            "mkdir -p {} && cat > {} && chmod {} {}",
            quote(parent(&file.remote)),
            quote(&file.remote),
            file.mode,
            quote(&file.remote)
        );
        let _ = on_machine(remote, &script, &bytes)?;
        said(format!("copied {}", file.remote));
    }
    let old = on_machine(
        remote,
        &format!("cat {} 2>/dev/null || true", quote(&plan.config_file)),
        &[],
    )
    .unwrap_or_default();
    let new = new_config(&String::from_utf8_lossy(&old), plan);
    let fresh = format!("{}.new", plan.config_file);
    let _ = on_machine(
        remote,
        &format!(
            "mkdir -p {} && cat > {} && mv {} {}",
            quote(parent(&plan.config_file)),
            quote(&fresh),
            quote(&fresh),
            quote(&plan.config_file)
        ),
        new.as_bytes(),
    )?;
    said(format!("wrote {}", plan.config_file));
    let _ = on_machine(remote, &plan.restart, &[])?;
    said(format!("ran {}", plan.restart));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Value, new_config, parse_json, plan, remote_commands};

    const PAIRED: &str = r#"{"machine":"n150","listen":"0.0.0.0:19532","ca_fingerprint":"SHA256:aa","server_fingerprint":"SHA256:bb","remote_files":[{"local":"/home/u/.local/share/ferrix/lode-receive/.pki/n150/client.crt","remote":"/etc/ferrix/lode-upload/n150.crt","mode":"0644"},{"local":"/k/client.key","remote":"/etc/ferrix/lode-upload/n150.key","mode":"0600"}],"remote_config_file":"/etc/ferrix/lode-upload.conf","remote_config_line":"n150 https://nazuna:19532/upload export ca=/etc/ferrix/lode-upload/ca.crt cert=/etc/ferrix/lode-upload/n150.crt key=/etc/ferrix/lode-upload/n150.key","remote_restart":"svc restart lode-upload.service"}"#;

    #[test]
    fn pairing_is_read_and_shown_as_the_commands_it_will_run() {
        let value = parse_json(PAIRED).expect("JSON");
        let plan = plan(&value).expect("a plan");
        assert_eq!(plan.files.len(), 2);
        assert_eq!(plan.files[1].mode, "0600");
        let shown = remote_commands(&plan);
        assert_eq!(shown.len(), 4);
        assert!(shown[0].starts_with(
            "mkdir -p '/etc/ferrix/lode-upload' && cat > '/etc/ferrix/lode-upload/n150.crt'"
        ));
        assert_eq!(shown[3], "svc restart lode-upload.service");
        // lode-receive's restart since it enables the service too.
        let enabling = PAIRED.replace(
            "\"svc restart lode-upload.service\"",
            "\"svc enable lode-upload.service && svc restart lode-upload.service\"",
        );
        assert!(plan_of(&enabling).is_some_and(|plan| plan.restart.starts_with("svc enable")));
        assert_eq!(
            parse_json("[1, true, null, \"a\\u00e9\"]"),
            Some(Value::Array(vec![
                Value::Number("1".to_owned()),
                Value::Bool(true),
                Value::Null,
                Value::Str("a\u{e9}".to_owned()),
            ]))
        );
        // A line for another machine, or a path that could break out of
        // the quotes, is refused.
        assert!(
            plan_of(&PAIRED.replace(
                "\"remote_config_line\":\"n150 ",
                "\"remote_config_line\":\"other "
            ))
            .is_none()
        );
        assert!(
            plan_of(&PAIRED.replace("/etc/ferrix/lode-upload/n150.key", "/etc/x'; rm -rf /"))
                .is_none()
        );
    }

    fn plan_of(text: &str) -> Option<super::Plan> {
        parse_json(text).and_then(|value| plan(&value))
    }

    #[test]
    fn the_machines_line_replaces_its_old_one_and_keeps_the_rest() {
        let plan = plan_of(PAIRED).expect("a plan");
        let old = "# uploads\nother https://x/upload\nn150 https://old/upload\nn150 https://older/upload\n";
        let new = new_config(old, &plan);
        assert_eq!(new.lines().count(), 3);
        assert!(new.contains("other https://x/upload"));
        assert!(new.contains("n150 https://nazuna:19532/upload"));
        assert!(!new.contains("old/upload"));
        assert_eq!(new_config("", &plan).lines().count(), 1);
    }
}
