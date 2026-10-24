//! hyprix's IPC, which is Hyprland's: `.socket.sock` answers one request a
//! connection (`j/workspaces` in JSON, `dispatch workspace 2`), and
//! `.socket2.sock` says each event as a line (`workspace>>2`). Both are in
//! `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/`, or under
//! `/tmp/hypr` where the first is not there, as Hyprland's
//! `getSocketFolder` has them.

use std::io::{Read as _, Write as _};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The compositor's sockets.
#[derive(Clone, Debug)]
pub struct Hypr {
    /// The folder they are in.
    folder: PathBuf,
}

/// The folder Hyprland's sockets are in, from `XDG_RUNTIME_DIR` and the
/// instance's signature.
#[must_use]
pub fn socket_folder(
    runtime: Option<&str>,
    signature: &str,
    exists: &dyn Fn(&Path) -> bool,
) -> PathBuf {
    match runtime {
        Some(dir) if !dir.is_empty() && exists(&Path::new(dir).join("hypr")) => {
            Path::new(dir).join("hypr").join(signature)
        }
        _ => Path::new("/tmp/hypr").join(signature),
    }
}

impl Hypr {
    /// The compositor this program was started under, or `None` outside
    /// one.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
        let runtime = std::env::var("XDG_RUNTIME_DIR").ok();
        Some(Hypr {
            folder: socket_folder(runtime.as_deref(), &signature, &Path::exists),
        })
    }

    /// A request's answer, or `None` when the compositor did not answer.
    #[must_use]
    pub fn ask(&self, request: &str) -> Option<String> {
        let mut stream = UnixStream::connect(self.folder.join(".socket.sock")).ok()?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        stream.write_all(request.as_bytes()).ok()?;
        let _ = stream.shutdown(std::net::Shutdown::Write);
        let mut answer = String::new();
        stream.read_to_string(&mut answer).ok()?;
        Some(answer)
    }

    /// Run a dispatcher, `workspace 2`; whether the compositor said `ok`.
    pub fn dispatch(&self, what: &str) -> bool {
        self.ask(&format!("dispatch {what}"))
            .is_some_and(|said| said.trim() == "ok")
    }

    /// Set an option for this session, `decoration:dim_inactive 1`.
    pub fn keyword(&self, what: &str) -> bool {
        self.ask(&format!("keyword {what}"))
            .is_some_and(|said| said.trim() == "ok")
    }

    /// The event socket, its lines to be read as they come.
    #[must_use]
    pub fn events(&self) -> Option<UnixStream> {
        let stream = UnixStream::connect(self.folder.join(".socket2.sock")).ok()?;
        stream.set_nonblocking(true).ok()?;
        Some(stream)
    }
}

/// The events after which the dock asks again: what changes a workspace,
/// a window's place or the focus.
pub const EVENTS: [&str; 10] = [
    "workspace",
    "workspacev2",
    "focusedmon",
    "openwindow",
    "closewindow",
    "movewindow",
    "movewindowv2",
    "createworkspace",
    "destroyworkspace",
    "activewindow",
];

/// Whether a line of the event socket is one the dock follows.
#[must_use]
pub fn wanted(line: &str) -> bool {
    line.split_once(">>")
        .is_some_and(|(name, _)| EVENTS.contains(&name))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{socket_folder, wanted};

    #[test]
    fn the_sockets_fall_back_to_tmp() {
        assert_eq!(
            socket_folder(Some("/run/user/0"), "abc", &|_| true),
            Path::new("/run/user/0/hypr/abc")
        );
        assert_eq!(
            socket_folder(Some("/run/user/0"), "abc", &|_| false),
            Path::new("/tmp/hypr/abc")
        );
        assert_eq!(
            socket_folder(None, "abc", &|_| true),
            Path::new("/tmp/hypr/abc")
        );
    }

    #[test]
    fn follows_the_events_that_move_things() {
        assert!(wanted("workspace>>2"));
        assert!(wanted("openwindow>>80a6f50,2,foot,foot"));
        assert!(!wanted("activelayout>>kbd,us"));
        assert!(!wanted("garbage"));
    }
}
