use super::{bad, Indexed, Result, METADATA_BUDGET};
use crate::archive::Entry;
use nsis::{
    decompress::CompressionMethod,
    installer::Instruction,
    strings::{NsisString, StringSegment},
};
use std::collections::HashMap;

fn path(value: NsisString, directory: bool) -> Result<Option<String>> {
    for (index, segment) in value.segments.iter().enumerate() {
        match segment {
            StringSegment::Literal(_) => {}
            StringSegment::Variable { name, .. } if index == 0 && name.as_ref() == "$INSTDIR" => {}
            _ => return Err(bad("runtime-dependent installer paths are unsupported")),
        }
    }
    let rooted = matches!(value.segments.first(), Some(StringSegment::Variable { name, .. }) if name.as_ref() == "$INSTDIR");
    let raw = value.to_string().replace('\\', "/");
    let relative = if rooted {
        let tail = raw
            .strip_prefix("$INSTDIR")
            .ok_or_else(|| bad("invalid install directory reference"))?;
        if tail.is_empty() {
            ""
        } else {
            tail.strip_prefix('/')
                .ok_or_else(|| bad("ambiguous install directory suffix"))?
        }
    } else {
        raw.as_str()
    };
    let relative = if directory {
        relative.trim_end_matches('/')
    } else {
        relative
    };
    if relative.is_empty() && directory && rooted {
        return Ok(None);
    }
    if relative.is_empty()
        || relative.starts_with('/')
        || relative.len() > 65536
        || relative.contains(['\0', ':'])
    {
        return Err(bad(format!(
            "unsafe or oversized installer path: {relative:?}"
        )));
    }
    let mut depth = 0;
    for component in relative.split('/') {
        depth += 1;
        let stem = component
            .split('.')
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        let device = matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$"
        ) || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(
                    suffix,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            });
        if depth > 128
            || component.is_empty()
            || matches!(component, "." | "..")
            || component.ends_with(['.', ' '])
            || device
            || component
                .chars()
                .any(|c| c.is_control() || matches!(c, '<' | '>' | '"' | '|' | '?' | '*'))
        {
            return Err(bad(format!(
                "unsafe or ambiguous installer path: {relative:?}"
            )));
        }
    }
    Ok(Some(relative.to_owned()))
}

pub(super) fn build(
    metadata: &nsis::NsisInstaller<'_>,
    metadata_size: usize,
    base: u64,
    end: u64,
) -> Result<Vec<Indexed>> {
    let mut output: Vec<Indexed> = Vec::new();
    let mut names: HashMap<String, bool> = HashMap::new();
    let mut budget = metadata_size;
    let mut instructions = metadata.instructions();
    while let Some(instruction) = instructions.next() {
        let instruction = instruction.map_err(|e| bad(e.to_string()))?;
        let (name, directory, start) = match instruction {
            Instruction::File(file) => (
                path(file.dest_path().map_err(|e| bad(e.to_string()))?, false)?,
                false,
                base.checked_add(u64::from(file.data_block_offset()))
                    .ok_or_else(|| bad("file offset overflow"))?,
            ),
            Instruction::Other(entry)
                if metadata.normalize_opcode(entry.which()) == nsis::opcode::EW_CREATEDIR =>
            {
                let value = instructions
                    .directory_path(&entry)
                    .map_err(|e| bad(e.to_string()))?;
                (path(value, true)?, true, 0)
            }
            _ => continue,
        };
        let Some(name) = name else {
            continue;
        };
        if !directory && start.checked_add(4).is_none_or(|offset| offset > end) {
            return Err(bad("file length prefix exceeds installer bounds"));
        }
        let folded = name.to_lowercase();
        if let Some(&prior_directory) = names.get(&folded) {
            if directory && prior_directory {
                continue;
            }
            return Err(bad(format!("ambiguous duplicate installer path: {name}")));
        }
        budget = budget
            .checked_add(std::mem::size_of::<Indexed>() + 192 + name.len() * 4 + folded.len() * 2)
            .filter(|&value| value <= METADATA_BUDGET)
            .ok_or_else(|| bad("installer index exceeds 64 MiB metadata budget"))?;
        names
            .try_reserve(1)
            .map_err(|_| bad("cannot allocate installer name index"))?;
        output
            .try_reserve(1)
            .map_err(|_| bad("cannot allocate installer entry index"))?;
        names.insert(folded, directory);
        output.push(Indexed {
            entry: Entry {
                path: name,
                size: 0,
                compressed_size: 0,
                crc32: None,
                modified: None,
                is_dir: directory,
            },
            start,
            encoded_size: 0,
            method: CompressionMethod::None,
            fingerprint: [0; 32],
        });
    }
    // Check every parent after collecting names: a file may appear after
    // its children, and unrelated names can lie between them when sorted.
    for name in names.keys() {
        for (offset, _) in name.match_indices('/') {
            if names.get(&name[..offset]) == Some(&false) {
                return Err(bad("installer file conflicts with a child path"));
            }
        }
    }
    Ok(output)
}
