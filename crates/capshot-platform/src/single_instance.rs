//! One capshot per user. A second launch hands its arguments to the running
//! instance over a local socket and exits.
//!
//! On Unix the socket is a file in a per-user directory (`$XDG_RUNTIME_DIR`,
//! mode 0700, or the per-user `$TMPDIR` on macOS), so other local users can
//! neither take the name nor send commands. On Windows it is a named pipe whose
//! name includes the user name.

use interprocess::local_socket::traits::{ListenerExt, Stream as _};
use interprocess::local_socket::{ListenerOptions, Name, Stream};
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::mpsc::Sender;

/// Longest argument message accepted from another launch.
const MAX_MESSAGE: u64 = 64 * 1024;

fn socket_name() -> std::io::Result<Name<'static>> {
    #[cfg(windows)]
    {
        use interprocess::local_socket::{GenericNamespaced, ToNsName};
        let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".into());
        format!("capshot-{user}.sock").to_ns_name::<GenericNamespaced>()
    }
    #[cfg(not(windows))]
    {
        use interprocess::local_socket::{GenericFilePath, ToFsName};
        let dir = std::env::var_os("XDG_RUNTIME_DIR")
            .or_else(|| std::env::var_os("TMPDIR"))
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
        dir.join(format!("capshot-{user}.sock")).to_fs_name::<GenericFilePath>().map(|n| n.into_owned())
    }
}

pub enum Instance {
    /// This process is the first one; messages from later launches arrive on the channel.
    Primary,
    /// Another instance is running and has received our arguments.
    Forwarded,
}

/// Try to become the primary instance. Each later launch's arguments arrive on
/// `tx` as one message (an empty list means "just show the window").
pub fn acquire(args: &[String], tx: Sender<Vec<String>>) -> std::io::Result<Instance> {
    let name = socket_name()?;
    if let Ok(mut stream) = Stream::connect(name.clone()) {
        writeln!(stream, "{}", args.join("\u{1f}"))?;
        stream.flush()?;
        return Ok(Instance::Forwarded);
    }
    // Nobody answered, so a leftover socket file (after a crash) is stale.
    let listener = ListenerOptions::new().name(name).try_overwrite(true).create_sync()?;
    std::thread::Builder::new().name("capshot-instance".into()).spawn(move || {
        for conn in listener.incoming().filter_map(Result::ok) {
            let tx = tx.clone();
            // One short-lived thread per launch, so a client that never sends a
            // newline cannot block later launches.
            let _ = std::thread::Builder::new().name("capshot-instance-msg".into()).spawn(move || {
                let mut line = String::new();
                if BufReader::new(conn.take(MAX_MESSAGE)).read_line(&mut line).is_ok() {
                    let _ = tx.send(parse_message(&line));
                }
            });
        }
    })?;
    Ok(Instance::Primary)
}

fn parse_message(line: &str) -> Vec<String> {
    line.trim_end_matches(['\r', '\n']).split('\u{1f}').filter(|s| !s.is_empty()).map(String::from).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn message_round_trip() {
        let args = ["--region".to_string(), "C:\\a b.png".to_string()];
        assert_eq!(super::parse_message(&format!("{}\n", args.join("\u{1f}"))), args);
        assert!(super::parse_message("\n").is_empty());
    }
}
