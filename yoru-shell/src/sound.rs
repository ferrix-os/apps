//! The quick settings' volume and microphone: the sound server's default
//! sink and source over the `PulseAudio` native protocol, which Ferrix's
//! own `pulsed` speaks (Ferrix's `docs/AUDIO.md`, U2) as PulseAudio and
//! PipeWire do. A connection a question: `AUTH`, `SET_CLIENT_NAME`, then
//! the lists, or a volume or mute set.
//!
//! A percentage is the device's volume over `PA_VOLUME_NORM`, as
//! pavucontrol and waybar count it; a volume set is set on every channel.

use std::ffi::CString;
use std::io::BufReader;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use pulseaudio::protocol::{
    self, AuthParams, AuthReply, ChannelVolume, Command, Props, SetClientNameReply,
    SetDeviceMuteParams, SetDeviceVolumeParams, SinkInfoList, SourceInfoList, Volume,
};

/// A sink or a source, as the quick settings show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// The server's index of it.
    pub index: u32,
    /// Its volume, a percentage of the norm.
    pub percent: u32,
    /// Muted.
    pub muted: bool,
    /// How many channels its volume has.
    pub channels: u8,
}

/// Where the server listens: `$PULSE_SERVER` (`unix:PATH`), else
/// `$XDG_RUNTIME_DIR/pulse/native`, else the desktop's
/// `/run/pulse/native`.
#[must_use]
pub fn socket(server: Option<&str>, runtime: Option<&str>) -> PathBuf {
    if let Some(path) = server.and_then(|s| s.strip_prefix("unix:")) {
        return PathBuf::from(path);
    }
    match runtime {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir).join("pulse/native"),
        _ => PathBuf::from("/run/pulse/native"),
    }
}

/// The percentage `volume` is of the norm, rounded.
#[must_use]
pub fn percent(volume: &ChannelVolume) -> u32 {
    let channels = volume.channels();
    if channels.is_empty() {
        return 0;
    }
    let sum: u64 = channels.iter().map(|v| u64::from(v.as_u32())).sum();
    let mean = sum / channels.len() as u64;
    let norm = u64::from(Volume::NORM.as_u32());
    u32::try_from((mean * 100 + norm / 2) / norm).unwrap_or(u32::MAX)
}

/// `percent` of the norm on each of `channels`.
#[must_use]
pub fn volume(percent: u32, channels: u8) -> ChannelVolume {
    let raw = u64::from(Volume::NORM.as_u32()) * u64::from(percent.min(150)) / 100;
    let one = Volume::from_u32_clamped(u32::try_from(raw).unwrap_or(u32::MAX));
    let mut out = ChannelVolume::empty();
    for _ in 0..channels.max(1) {
        out.push(one);
    }
    out
}

/// A connection to the server, past `AUTH` and its name.
struct Link {
    writer: UnixStream,
    reader: BufReader<UnixStream>,
    version: u16,
    seq: u32,
}

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

impl Link {
    fn open() -> Result<Link, String> {
        let path = socket(
            std::env::var("PULSE_SERVER").ok().as_deref(),
            std::env::var("XDG_RUNTIME_DIR").ok().as_deref(),
        );
        let stream = UnixStream::connect(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
        let mut link = Link {
            writer: stream.try_clone().map_err(err)?,
            reader: BufReader::new(stream),
            version: protocol::MAX_VERSION,
            seq: 0,
        };
        link.send(&Command::Auth(AuthParams {
            version: protocol::MAX_VERSION,
            supports_shm: false,
            supports_memfd: false,
            cookie: vec![0; 256],
        }))?;
        let (_, auth) = protocol::read_reply_message::<AuthReply>(&mut link.reader, link.version)
            .map_err(err)?;
        link.version = auth.version;
        let mut props = Props::new();
        props.set(protocol::Prop::ApplicationName, c"yoru-shell");
        link.send(&Command::SetClientName(props))?;
        let _ = protocol::read_reply_message::<SetClientNameReply>(&mut link.reader, link.version)
            .map_err(err)?;
        Ok(link)
    }

    fn send(&mut self, command: &Command) -> Result<(), String> {
        self.seq += 1;
        protocol::write_command_message(&mut self.writer, self.seq, command, self.version)
            .map_err(err)
    }

    fn ack(&mut self) -> Result<(), String> {
        protocol::read_ack_message(&mut self.reader)
            .map(|_| ())
            .map_err(err)
    }
}

/// The first sink and the first source, as the server lists them; `Err`
/// when there is no server to ask.
///
/// # Errors
///
/// No server, or one that answered with something else.
pub fn devices() -> Result<(Option<Device>, Option<Device>), String> {
    let mut link = Link::open()?;
    link.send(&Command::GetSinkInfoList)?;
    let (_, sinks) = protocol::read_reply_message::<SinkInfoList>(&mut link.reader, link.version)
        .map_err(err)?;
    link.send(&Command::GetSourceInfoList)?;
    let (_, sources) =
        protocol::read_reply_message::<SourceInfoList>(&mut link.reader, link.version)
            .map_err(err)?;
    let device = |index, cvolume: &ChannelVolume, muted| Device {
        index,
        percent: percent(cvolume),
        muted,
        channels: u8::try_from(cvolume.channels().len()).unwrap_or(2),
    };
    // A sink's monitor is a source too, in PulseAudio; it is no microphone.
    let source = sources
        .iter()
        .find(|s| s.monitor_of_sink_index.is_none())
        .map(|s| device(s.index, &s.cvolume, s.muted));
    Ok((
        sinks.first().map(|s| device(s.index, &s.cvolume, s.muted)),
        source,
    ))
}

/// Which device a change is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    /// The sink: the volume.
    Sink,
    /// The source: the microphone.
    Source,
}

/// Set `device`'s volume to `percent`.
///
/// # Errors
///
/// No server, or it refused.
pub fn set_volume(which: Which, device: &Device, percent: u32) -> Result<(), String> {
    let mut link = Link::open()?;
    let params = SetDeviceVolumeParams {
        device_index: Some(device.index),
        device_name: None::<CString>,
        volume: volume(percent, device.channels),
    };
    link.send(&match which {
        Which::Sink => Command::SetSinkVolume(params),
        Which::Source => Command::SetSourceVolume(params),
    })?;
    link.ack()
}

/// Mute `device`, or unmute it.
///
/// # Errors
///
/// No server, or it refused.
pub fn set_mute(which: Which, device: &Device, mute: bool) -> Result<(), String> {
    let mut link = Link::open()?;
    let params = SetDeviceMuteParams {
        device_index: Some(device.index),
        device_name: None::<CString>,
        mute,
    };
    link.send(&match which {
        Which::Sink => Command::SetSinkMute(params),
        Which::Source => Command::SetSourceMute(params),
    })?;
    link.ack()
}

#[cfg(test)]
mod tests {
    use super::{percent, socket, volume};

    #[test]
    fn the_socket_is_the_servers_the_sessions_or_the_desktops() {
        assert_eq!(
            socket(Some("unix:/run/pulse/native"), Some("/r")),
            std::path::PathBuf::from("/run/pulse/native")
        );
        assert_eq!(
            socket(None, Some("/r/1000")),
            std::path::PathBuf::from("/r/1000/pulse/native")
        );
        assert_eq!(
            socket(None, None),
            std::path::PathBuf::from("/run/pulse/native")
        );
    }

    #[test]
    fn a_percentage_is_of_the_norm_on_every_channel() {
        let half = volume(50, 2);
        assert_eq!(half.channels().len(), 2);
        assert_eq!(percent(&half), 50);
        assert_eq!(percent(&volume(100, 2)), 100);
        assert_eq!(percent(&volume(0, 1)), 0);
        assert_eq!(percent(&volume(400, 2)), 150, "held at 150%");
    }
}
