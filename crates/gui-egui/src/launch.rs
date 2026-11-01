//! OS launch requests. Paths retain their native encoding and never pass through a shell.
use std::{ffi::OsString, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Open,
    Compress,
    CompressZip,
    Extract,
    ExtractHere,
    ExtractFolder,
}
impl Action {
    pub const MENU: [Self; 5] = [
        Self::Compress,
        Self::CompressZip,
        Self::Extract,
        Self::ExtractHere,
        Self::ExtractFolder,
    ];
    pub fn argument(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Compress => "compress",
            Self::CompressZip => "compress-zip",
            Self::Extract => "extract",
            Self::ExtractHere => "extract-here",
            Self::ExtractFolder => "extract-folder",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        std::iter::once(Self::Open)
            .chain(Self::MENU)
            .find(|action| action.argument() == value)
    }
}

#[derive(Debug, Default)]
pub enum Request {
    #[default]
    Empty,
    Paths(Action, Vec<PathBuf>),
    Help,
    Version,
    Embedding,
    ActionFile(PathBuf),
}
pub fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Request, String> {
    let mut arguments = arguments.into_iter().peekable();
    let mut action = Action::Open;
    let mut explicit = false;
    let mut paths = Vec::new();
    while let Some(argument) = arguments.next() {
        if argument == "--" {
            paths.extend(arguments.map(PathBuf::from));
            break;
        }
        if !paths.is_empty() {
            paths.push(argument.into());
            continue;
        }
        match argument.to_str() {
            Some("--help" | "-h") => return Ok(Request::Help),
            Some("--version") => return Ok(Request::Version),
            Some(value)
                if value.eq_ignore_ascii_case("-embedding")
                    || value.eq_ignore_ascii_case("/embedding") =>
            {
                return Ok(Request::Embedding)
            }
            Some("--shell-action-file") => {
                let file = arguments.next().ok_or("Missing action file")?;
                if arguments.next().is_some() {
                    return Err("Unexpected arguments after action file".into());
                }
                return Ok(Request::ActionFile(file.into()));
            }
            Some("--shell-action") if !explicit => {
                action = arguments
                    .next()
                    .as_deref()
                    .and_then(|s| s.to_str())
                    .and_then(Action::parse)
                    .ok_or("Unknown or missing shell action")?;
                explicit = true;
            }
            Some(value) if value.starts_with('-') => {
                return Err(format!("Unknown option: {value}"))
            }
            _ => paths.push(argument.into()),
        }
    }
    if paths.is_empty() {
        if explicit {
            Err("Select at least one file or folder".into())
        } else {
            Ok(Request::Empty)
        }
    } else if matches!(
        action,
        Action::Extract | Action::ExtractHere | Action::ExtractFolder
    ) && paths.len() != 1
    {
        Err("Extraction shortcuts accept one archive at a time".into())
    } else {
        Ok(Request::Paths(action, paths))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shell_selection_and_option_like_paths_are_preserved() {
        let Request::Paths(action, paths) = parse(
            [
                "--shell-action",
                "compress",
                "--",
                "中文 空格.txt",
                "--help",
                "%*",
            ]
            .map(OsString::from),
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(action, Action::Compress);
        assert_eq!(paths, ["中文 空格.txt", "--help", "%*"].map(PathBuf::from));
        assert!(parse(["--shell-action", "extract-here"].map(OsString::from)).is_err());
        assert!(
            parse(["--shell-action", "extract", "a.zip", "b.zip"].map(OsString::from)).is_err()
        );
        assert!(matches!(
            parse(["-Embedding".into()]).unwrap(),
            Request::Embedding
        ));
    }
    #[cfg(unix)]
    #[test]
    fn native_non_utf8_paths_round_trip() {
        use std::os::unix::ffi::OsStringExt;
        let path = OsString::from_vec(vec![b'a', 0xff]);
        let Request::Paths(_, paths) = parse([path.clone()]).unwrap() else {
            panic!()
        };
        assert_eq!(paths, [PathBuf::from(path)]);
    }
}
