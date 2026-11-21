//! Files used by the Nova launcher's mod switches.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs::{self, File},
    io::{ErrorKind, Read, Write},
    path::Path,
};
use zip::ZipArchive;

const MANIFEST: &str = "horizon-mod.toml";
const CONFIG: &str = "horizon-mods.txt";
type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Deserialize)]
struct Manifest {
    id: String,
    name: Option<String>,
}

#[derive(Serialize)]
struct Mod {
    id: String,
    name: String,
    enabled: bool,
}

#[derive(Serialize)]
struct Inventory {
    mods: Vec<Mod>,
    problems: Vec<String>,
}

fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn manifest(reader: impl Read) -> Result<Manifest> {
    // Same manifest limit as horizon-mod-package. Never unpack the mod's payload.
    const LIMIT: u64 = 32 << 20;
    let mut text = String::new();
    reader.take(LIMIT + 1).read_to_string(&mut text)?;
    if text.len() as u64 > LIMIT {
        return Err("Mod manifest exceeds 32 MiB".into());
    }
    let info: Manifest = toml::from_str(&text)?;
    if !valid_id(&info.id) {
        return Err("Invalid mod ID".into());
    }
    Ok(info)
}

fn read_manifest(path: &Path) -> Result<Manifest> {
    if path.is_dir() {
        return manifest(File::open(path.join(MANIFEST))?);
    }
    let mut archive = ZipArchive::new(File::open(path)?)?;
    let entry = archive.by_name(MANIFEST)?;
    manifest(entry)
}

fn config(home: &Path) -> Result<String> {
    match fs::read_to_string(home.join(CONFIG)) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(String::new()),
        other => Ok(other?),
    }
}

fn scan(home: &Path) -> Result<Inventory> {
    let folder = home.join("horizon-mods");
    fs::create_dir_all(&folder)?;
    let text = config(home)?;
    let enabled: BTreeSet<_> = text
        .lines()
        .filter_map(|line| line.split_once(':'))
        .filter(|(_, value)| value.trim() == "true")
        .filter_map(|(key, _)| key.trim().strip_prefix("mod."))
        .collect();
    let mut mods = BTreeMap::new();
    let mut problems = Vec::new();
    let mut paths = fs::read_dir(folder)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    for path in paths {
        if !path.extension().is_some_and(|ext| ext == "hmod") && !path.join(MANIFEST).is_file() {
            continue;
        }
        let file = path.file_name().unwrap_or_default().to_string_lossy();
        match read_manifest(&path) {
            Ok(info) if mods.contains_key(&info.id) => {
                problems.push(format!(
                    "{file} has the same ID ({}) as another installed mod.",
                    info.id
                ));
            }
            Ok(info) => {
                let name = info
                    .name
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| info.id.clone());
                mods.insert(
                    info.id.clone(),
                    Mod {
                        enabled: enabled.contains(info.id.as_str()),
                        id: info.id,
                        name,
                    },
                );
            }
            Err(_) => problems.push(format!("Could not read a mod ID from {file}.")),
        }
    }
    Ok(Inventory {
        mods: mods.into_values().collect(),
        problems,
    })
}

pub fn scan_json(home: &Path) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&scan(home)?)?)
}

pub fn set_enabled(home: &Path, id: &str, enabled: bool) -> Result<()> {
    if !valid_id(id) {
        return Err("Invalid mod ID".into());
    }
    // Re-read on every change so edits made in Files and unrelated settings survive.
    let text = config(home)?;
    let key = format!("mod.{id}");
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut updated = String::new();
    let mut replaced = false;
    for line in text.split_inclusive('\n') {
        if line.split_once(':').is_some_and(|(k, _)| k.trim() == key) {
            if !replaced {
                updated.push_str(&format!("{key}: {enabled}"));
                if line.ends_with("\r\n") {
                    updated.push_str("\r\n");
                } else if line.ends_with('\n') {
                    updated.push('\n');
                }
                replaced = true;
            }
        } else {
            updated.push_str(line);
        }
    }
    if !replaced {
        if !updated.is_empty() && !updated.ends_with('\n') {
            updated.push_str(newline);
        }
        updated.push_str(&format!("{key}: {enabled}{newline}"));
    }
    let mut temp = tempfile::NamedTempFile::new_in(home)?;
    temp.write_all(updated.as_bytes())?;
    temp.persist(home.join(CONFIG))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::{write::FileOptions, ZipWriter};

    #[test]
    fn scans_archives_and_directories_by_manifest_id() {
        let home = tempfile::tempdir().unwrap();
        assert!(scan(home.path()).unwrap().mods.is_empty());
        let folder = home.path().join("horizon-mods");
        fs::create_dir(folder.join("renamed")).unwrap();
        fs::write(
            folder.join("renamed").join(MANIFEST),
            "id='loose'\nname='Loose mod'",
        )
        .unwrap();
        let mut zip = ZipWriter::new(File::create(folder.join("different-name.hmod")).unwrap());
        zip.start_file(
            MANIFEST,
            FileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(b"name='Packed mod'\nid='packed'\n[dependencies]\nid='other'")
            .unwrap();
        zip.finish().unwrap();
        fs::write(folder.join("broken.hmod"), "invalid").unwrap();
        set_enabled(home.path(), "packed", true).unwrap();
        let list = scan(home.path()).unwrap();
        assert_eq!(list.mods.len(), 2);
        assert_eq!(list.mods[0].id, "loose");
        assert!(!list.mods[0].enabled);
        assert_eq!(list.mods[1].id, "packed");
        assert!(list.mods[1].enabled);
        assert_eq!(list.problems.len(), 1);
    }

    #[test]
    fn toggling_preserves_other_settings_and_removes_conflicting_lines() {
        let home = tempfile::tempdir().unwrap();
        let file = home.path().join(CONFIG);
        let original = "# keep\r\n mod.a :true\r\nmod.ab:true\r\nmod.a:false\r\nkey.a.jump:space";
        fs::write(&file, original).unwrap();
        set_enabled(home.path(), "a", false).unwrap();
        set_enabled(home.path(), "new", true).unwrap();
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "# keep\r\nmod.a: false\r\nmod.ab:true\r\nkey.a.jump:space\r\nmod.new: true\r\n"
        );
        fs::write(&file, [0xff]).unwrap();
        assert!(set_enabled(home.path(), "a", true).is_err());
        assert_eq!(fs::read(&file).unwrap(), [0xff]);
    }
}
