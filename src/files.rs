// File copies, folder walks and checksums.
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

// --- Files ---
pub fn copy_tree(from: &Path, to: &Path, overwrite: bool) -> Result<u64> {
    let mut count = 0;
    if !from.exists() {
        return Ok(0);
    }
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)? {
        let e = e?;
        let target = to.join(e.file_name());
        let kind = e.file_type()?;
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            count += copy_tree(&e.path(), &target, overwrite)?;
        } else if overwrite || !target.exists() {
            fs::copy(e.path(), &target).with_context(|| format!("Copie impossible : {}", e.path().display()))?;
            count += 1;
        }
    }
    Ok(count)
}

pub fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(kind) = e.file_type() else { continue };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            walk_files(&e.path(), out);
        } else {
            out.push(e.path());
        }
    }
}

pub fn tree_stats(dir: &Path) -> (u64, u64) {
    let mut files = vec![];
    walk_files(dir, &mut files);
    let size = files.iter().filter_map(|f| f.metadata().ok()).map(|m| m.len()).sum();
    (files.len() as u64, size)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02X}")).collect()
}

pub fn sha256_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut h = Sha256::new();
    let mut file = fs::File::open(path)?;
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 { break; }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02X}")).collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn checksums_match_the_standard() {
        let empty = "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855";
        assert_eq!(super::sha256_hex(b""), empty);
        let f = std::env::temp_dir().join(format!("forkbay-sha-{}", std::process::id()));
        std::fs::write(&f, vec![b'a'; 300 * 1024]).unwrap();
        let whole = super::sha256_hex(&std::fs::read(&f).unwrap());
        assert_eq!(super::sha256_file(&f).unwrap(), whole);
        std::fs::remove_file(f).unwrap();
    }
}
