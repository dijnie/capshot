//! Command-line arguments: `capshot [--fullscreen|--region|--window] [IMAGE]`.

use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureMode {
    Fullscreen,
    Region,
    Window,
}

#[derive(Debug, Default, PartialEq)]
pub struct Cli {
    pub capture: Option<CaptureMode>,
    pub open: Option<PathBuf>,
}

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Cli {
    let mut cli = Cli::default();
    for a in args {
        match a.to_str() {
            Some("--fullscreen") => cli.capture = Some(CaptureMode::Fullscreen),
            Some("--region") => cli.capture = Some(CaptureMode::Region),
            Some("--window") => cli.capture = Some(CaptureMode::Window),
            Some(s) if s.starts_with("--") => eprintln!("capshot: ignoring unknown option {s}"),
            _ => cli.open = Some(PathBuf::from(a)),
        }
    }
    cli
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    #[test]
    fn parses_modes_and_path() {
        assert_eq!(parse(args(&["--region"])).capture, Some(CaptureMode::Region));
        let c = parse(args(&["--bogus", "shot.png"]));
        assert_eq!(c.capture, None);
        assert_eq!(c.open, Some(PathBuf::from("shot.png")));
        assert_eq!(parse(args(&[])), Cli::default());
    }
}
