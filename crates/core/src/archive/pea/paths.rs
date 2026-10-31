use super::{checksum::bad, Indexed};
use crate::error::GeeZipResult as Result;

/// Native PEA stores absolute qualified names. Check every original component
/// before dropping the common parent; traversal must never disappear in trimming.
pub(super) fn normalize(entries: &mut [Indexed]) -> Result<()> {
    let mut origin = None;
    let mut common = None::<String>;
    for item in entries.iter_mut() {
        let name = item.entry.path.replace('\\', "/");
        let absolute = name.starts_with('/');
        let mut canonical = String::with_capacity(name.len());
        let mut drive = None;
        let mut depth = 0;
        for component in name.split('/').filter(|value| !value.is_empty()) {
            depth += 1;
            if depth > 128 {
                return Err(bad("filename exceeds 128 components"));
            }
            if component == "." || component == ".." {
                return Err(bad("unsafe dot component in qualified filename"));
            }
            if canonical.is_empty()
                && component.len() == 2
                && component.as_bytes()[0].is_ascii_alphabetic()
                && component.ends_with(':')
            {
                let letter = component.as_bytes()[0].to_ascii_uppercase();
                drive = Some(letter);
                canonical.push(letter as char);
                canonical.push(':');
                continue;
            }
            if component.contains(':') {
                return Err(bad("unsupported colon in filename"));
            }
            if !canonical.is_empty() {
                canonical.push('/');
            }
            canonical.push_str(component);
        }
        if canonical.is_empty() || canonical.ends_with(':') {
            return Err(bad("root-only filename"));
        }
        let current = (absolute, drive);
        if origin.is_some_and(|value| value != current) {
            return Err(bad("unsupported mixture of filename roots"));
        }
        origin = Some(current);
        let parent = canonical.rsplit_once('/').map_or("", |pair| pair.0);
        if let Some(prefix) = common.as_mut() {
            if !prefix.is_empty() {
                let bytes = prefix
                    .split('/')
                    .zip(parent.split('/'))
                    .take_while(|(a, b)| a == b)
                    .map(|(part, _)| part.len() + 1)
                    .sum::<usize>()
                    .saturating_sub(1);
                prefix.truncate(bytes);
            }
        } else {
            common = Some(parent.to_owned());
        }
        item.entry.path = canonical;
    }
    let trim = common
        .filter(|name| !name.is_empty())
        .map_or(0, |name| name.len() + 1);
    if trim != 0 {
        for item in entries {
            item.entry.path.drain(..trim);
        }
    }
    Ok(())
}
