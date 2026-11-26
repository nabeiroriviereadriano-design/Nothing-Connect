//! Moteur de transfert (E/S synchrones, à exécuter dans un thread bloquant).
//!
//! * **Envoi** : [`plan`] parcourt fichiers et dossiers, [`OutgoingFile`] lit par blocs à
//!   partir d'un offset (reprise) en calculant le SHA-256 du fichier entier.
//! * **Réception** : [`IncomingFile`] écrit dans `<dest>/.nc-partial/<id>-<n>.ncpart`,
//!   reprend à la taille déjà reçue, vérifie le SHA-256, puis déplace le fichier final en
//!   appliquant la [`ConflictPolicy`]. Un fichier n'est déclaré terminé qu'après
//!   vérification.
//! * Les chemins reçus sont **assainis** : impossible d'écrire hors du dossier de destination.

use nc_protocol::OfferedFile;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

pub const PARTIAL_DIR: &str = ".nc-partial";
/// Nombre maximal de fichiers par transfert (protection contre les offres abusives).
pub const MAX_FILES: usize = 100_000;
const HASH_BUF: usize = 1 << 20;

#[derive(Debug, Error)]
pub enum TransferError {
    #[error("E/S : {0}")]
    Io(#[from] io::Error),
    #[error("chemin refusé : {0}")]
    BadPath(String),
    #[error("bloc hors séquence (attendu {expected}, reçu {got})")]
    OutOfOrder { expected: u64, got: u64 },
    #[error("le fichier dépasse la taille annoncée")]
    Overflow,
    #[error("empreinte SHA-256 différente : fichier corrompu")]
    HashMismatch,
    #[error("fichier incomplet ({got}/{expected} octets)")]
    Incomplete { got: u64, expected: u64 },
    #[error("le fichier source a changé pendant l'envoi")]
    SourceChanged,
    #[error("trop de fichiers")]
    TooManyFiles,
}

pub type Result<T> = std::result::Result<T, TransferError>;

/// Fichier à envoyer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub path: PathBuf,
    pub rel_path: String,
    pub size: u64,
    pub modified_ms: Option<i64>,
}

impl SourceFile {
    pub fn offered(&self) -> OfferedFile {
        OfferedFile {
            rel_path: self.rel_path.clone(),
            size: self.size,
            modified_ms: self.modified_ms,
        }
    }
}

/// Construit la liste des fichiers d'un envoi (les dossiers sont parcourus récursivement,
/// les liens symboliques ignorés pour ne pas sortir du dossier choisi).
pub fn plan(inputs: &[PathBuf]) -> Result<Vec<SourceFile>> {
    let named: Vec<(PathBuf, Option<String>)> = inputs.iter().map(|p| (p.clone(), None)).collect();
    plan_named(&named)
}

/// Comme [`plan`], avec un nom d'affichage optionnel par entrée (Android : un descripteur
/// `/proc/self/fd/N` ouvert depuis un `content://` porte le nom fourni par le système).
/// Les entrées choisies explicitement suivent les liens ; le contenu des dossiers, non.
pub fn plan_named(inputs: &[(PathBuf, Option<String>)]) -> Result<Vec<SourceFile>> {
    let mut out = Vec::new();
    for (p, name) in inputs {
        let meta = fs::metadata(p)?;
        let base_name = name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .or_else(|| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "fichier".into());
        let base_name = base_name.replace(['/', '\\'], "_");
        if meta.is_file() {
            out.push(source(p, base_name, &meta));
        } else if meta.is_dir() {
            walk(p, &base_name, &mut out)?;
        }
        if out.len() > MAX_FILES {
            return Err(TransferError::TooManyFiles);
        }
    }
    Ok(out)
}

fn source(p: &Path, rel: String, meta: &fs::Metadata) -> SourceFile {
    let modified_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64);
    SourceFile {
        path: p.to_path_buf(),
        rel_path: rel,
        size: meta.len(),
        modified_ms,
    }
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<SourceFile>) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let meta = match fs::symlink_metadata(e.path()) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let rel = format!("{prefix}/{}", e.file_name().to_string_lossy());
        if meta.is_file() {
            out.push(source(&e.path(), rel, &meta));
        } else if meta.is_dir() {
            walk(&e.path(), &rel, out)?;
        }
        if out.len() > MAX_FILES {
            return Err(TransferError::TooManyFiles);
        }
    }
    Ok(())
}

const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Transforme un chemin relatif reçu en chemin sûr sous `dest`.
pub fn safe_relative(rel: &str) -> Result<PathBuf> {
    if rel.is_empty() || rel.len() > 4096 {
        return Err(TransferError::BadPath(rel.into()));
    }
    let mut out = PathBuf::new();
    for part in rel.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || part.contains(':') || part.contains('\0') {
            return Err(TransferError::BadPath(rel.into()));
        }
        let mut clean: String = part
            .chars()
            .map(|c| {
                if matches!(c, '<' | '>' | '"' | '|' | '?' | '*') || c.is_control() {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        clean = clean.trim_end_matches([' ', '.']).to_string();
        if clean.is_empty() {
            clean = "_".into();
        }
        let stem = clean.split('.').next().unwrap_or("").to_ascii_uppercase();
        if WINDOWS_RESERVED.contains(&stem.as_str()) {
            clean.insert(0, '_');
        }
        if clean.len() > 255 {
            clean.truncate(255);
        }
        out.push(clean);
    }
    if out.as_os_str().is_empty() || out.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(TransferError::BadPath(rel.into()));
    }
    Ok(out)
}

/// Lecture d'un fichier à envoyer.
pub struct OutgoingFile {
    file: File,
    size: u64,
    pos: u64,
    hasher: Sha256,
}

impl OutgoingFile {
    /// Ouvre le fichier et positionne la lecture à `offset` (le préfixe est relu pour l'empreinte).
    pub fn open(src: &SourceFile, offset: u64) -> Result<Self> {
        let file = File::open(&src.path)?;
        Self::from_file(file, src.size, offset)
    }

    /// Variante pour un descripteur déjà ouvert (Android : contenu `content://`).
    pub fn from_file(mut file: File, size: u64, offset: u64) -> Result<Self> {
        let real = file.metadata()?.len();
        if real != size {
            return Err(TransferError::SourceChanged);
        }
        let offset = offset.min(size);
        file.seek(SeekFrom::Start(0))?;
        let mut hasher = Sha256::new();
        hash_prefix(&mut file, offset, &mut hasher)?;
        Ok(Self {
            file,
            size,
            pos: offset,
            hasher,
        })
    }

    pub fn position(&self) -> u64 {
        self.pos
    }

    pub fn is_done(&self) -> bool {
        self.pos >= self.size
    }

    /// Lit le prochain bloc dans `buf` (vidé) ; renvoie l'offset du bloc et sa taille.
    pub fn next_chunk(&mut self, buf: &mut Vec<u8>, max: usize) -> Result<Option<(u64, usize)>> {
        if self.is_done() {
            return Ok(None);
        }
        let want = ((self.size - self.pos) as usize).min(max);
        let start = buf.len();
        buf.resize(start + want, 0);
        self.file.read_exact(&mut buf[start..]).map_err(|e| {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                TransferError::SourceChanged
            } else {
                e.into()
            }
        })?;
        self.hasher.update(&buf[start..]);
        let off = self.pos;
        self.pos += want as u64;
        Ok(Some((off, want)))
    }

    /// Empreinte finale (à appeler après le dernier bloc).
    pub fn finish(self) -> String {
        hex::encode(self.hasher.finalize())
    }
}

fn hash_prefix(file: &mut File, len: u64, hasher: &mut Sha256) -> Result<()> {
    let mut left = len;
    let mut buf = vec![0u8; HASH_BUF];
    while left > 0 {
        let n = (left as usize).min(buf.len());
        file.read_exact(&mut buf[..n])?;
        hasher.update(&buf[..n]);
        left -= n as u64;
    }
    Ok(())
}

/// Politique en cas de fichier déjà présent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    /// Conserver les deux : `photo (1).jpg`.
    #[default]
    KeepBoth,
    Replace,
    /// Ne pas écrire le nouveau fichier.
    Skip,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOutcome {
    Saved(PathBuf),
    /// Fichier identique (même SHA-256) déjà présent : aucun doublon créé.
    AlreadyPresent(PathBuf),
    Skipped(PathBuf),
}

impl FileOutcome {
    pub fn path(&self) -> &Path {
        match self {
            FileOutcome::Saved(p) | FileOutcome::AlreadyPresent(p) | FileOutcome::Skipped(p) => p,
        }
    }
}

/// Chemin du fichier partiel d'un transfert.
pub fn partial_path(dest: &Path, id: &Uuid, idx: u32) -> PathBuf {
    dest.join(PARTIAL_DIR)
        .join(format!("{}-{idx}.ncpart", id.simple()))
}

/// Offsets de reprise pour une offre : taille déjà reçue de chaque fichier partiel.
pub fn resume_offsets(dest: &Path, id: &Uuid, files: &[OfferedFile]) -> Vec<u64> {
    files
        .iter()
        .enumerate()
        .map(
            |(i, f)| match fs::metadata(partial_path(dest, id, i as u32)) {
                Ok(m) if m.len() <= f.size => m.len(),
                _ => 0,
            },
        )
        .collect()
}

/// Réception d'un fichier.
pub struct IncomingFile {
    out: BufWriter<File>,
    partial: PathBuf,
    final_rel: PathBuf,
    dest: PathBuf,
    size: u64,
    pos: u64,
    hasher: Sha256,
}

impl IncomingFile {
    /// Prépare la réception (reprend un fichier partiel si `offset` > 0).
    pub fn open(
        dest: &Path,
        id: &Uuid,
        idx: u32,
        offered: &OfferedFile,
        offset: u64,
    ) -> Result<Self> {
        let final_rel = safe_relative(&offered.rel_path)?;
        let partial = partial_path(dest, id, idx);
        fs::create_dir_all(partial.parent().unwrap())?;
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&partial)?;
        let existing = file.metadata()?.len();
        let offset = if offset <= existing && offset <= offered.size {
            offset
        } else {
            0
        };
        file.set_len(offset)?;
        file.seek(SeekFrom::Start(0))?;
        let mut hasher = Sha256::new();
        hash_prefix(&mut file, offset, &mut hasher)?;
        file.seek(SeekFrom::Start(offset))?;
        Ok(Self {
            out: BufWriter::with_capacity(1 << 20, file),
            partial,
            final_rel,
            dest: dest.to_path_buf(),
            size: offered.size,
            pos: offset,
            hasher,
        })
    }

    pub fn position(&self) -> u64 {
        self.pos
    }

    pub fn write_chunk(&mut self, offset: u64, data: &[u8]) -> Result<()> {
        if offset != self.pos {
            return Err(TransferError::OutOfOrder {
                expected: self.pos,
                got: offset,
            });
        }
        if self.pos + data.len() as u64 > self.size {
            return Err(TransferError::Overflow);
        }
        self.out.write_all(data)?;
        self.hasher.update(data);
        self.pos += data.len() as u64;
        Ok(())
    }

    /// Vide les tampons sur disque (avant une pause, pour une reprise exacte).
    pub fn sync(&mut self) -> Result<()> {
        self.out.flush()?;
        self.out.get_ref().sync_data()?;
        Ok(())
    }

    /// Vérifie l'empreinte puis place le fichier à sa destination finale.
    pub fn finish(
        mut self,
        expected_sha256: &str,
        policy: ConflictPolicy,
    ) -> Result<(FileOutcome, String)> {
        if self.pos != self.size {
            return Err(TransferError::Incomplete {
                got: self.pos,
                expected: self.size,
            });
        }
        self.out.flush()?;
        self.out.get_ref().sync_all()?;
        let digest = hex::encode(self.hasher.clone().finalize());
        if !digest.eq_ignore_ascii_case(expected_sha256) {
            drop(self.out);
            let _ = fs::remove_file(&self.partial);
            return Err(TransferError::HashMismatch);
        }
        drop(self.out);
        let target = self.dest.join(&self.final_rel);
        if let Some(dir) = target.parent() {
            fs::create_dir_all(dir)?;
        }
        let outcome = if target.exists() {
            if file_sha256(&target).map(|h| h == digest).unwrap_or(false) {
                fs::remove_file(&self.partial)?;
                FileOutcome::AlreadyPresent(target)
            } else {
                match policy {
                    ConflictPolicy::Skip => {
                        fs::remove_file(&self.partial)?;
                        FileOutcome::Skipped(target)
                    }
                    ConflictPolicy::Replace => {
                        replace(&self.partial, &target)?;
                        FileOutcome::Saved(target)
                    }
                    ConflictPolicy::KeepBoth => {
                        let t = unique_name(&target);
                        fs::rename(&self.partial, &t)?;
                        FileOutcome::Saved(t)
                    }
                }
            }
        } else {
            fs::rename(&self.partial, &target)?;
            FileOutcome::Saved(target)
        };
        cleanup_partial_dir(&self.dest);
        Ok((outcome, digest))
    }

    /// Abandon (annulation) : supprime le fichier partiel.
    pub fn discard(self) {
        let partial = self.partial.clone();
        let dest = self.dest.clone();
        drop(self.out);
        let _ = fs::remove_file(partial);
        cleanup_partial_dir(&dest);
    }
}

/// Supprime les fichiers partiels d'un transfert annulé.
pub fn discard_partials(dest: &Path, id: &Uuid, count: usize) {
    for i in 0..count {
        let _ = fs::remove_file(partial_path(dest, id, i as u32));
    }
    cleanup_partial_dir(dest);
}

/// Nettoyage contrôlé : supprime les partiels plus vieux que `max_age` (transferts abandonnés).
pub fn purge_stale_partials(dest: &Path, max_age: std::time::Duration) -> usize {
    let dir = dest.join(PARTIAL_DIR);
    let mut n = 0;
    if let Ok(rd) = fs::read_dir(&dir) {
        for e in rd.flatten() {
            let old = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .map(|a| a > max_age)
                .unwrap_or(false);
            if old
                && e.path().extension().map(|x| x == "ncpart").unwrap_or(false)
                && fs::remove_file(e.path()).is_ok()
            {
                n += 1;
            }
        }
    }
    cleanup_partial_dir(dest);
    n
}

fn cleanup_partial_dir(dest: &Path) {
    let _ = fs::remove_dir(dest.join(PARTIAL_DIR)); // échoue (sans effet) s'il n'est pas vide
}

fn replace(from: &Path, to: &Path) -> io::Result<()> {
    // Sous Windows, rename échoue si la cible existe : on la supprime d'abord.
    if cfg!(windows) && to.exists() {
        fs::remove_file(to)?;
    }
    fs::rename(from, to)
}

fn unique_name(target: &Path) -> PathBuf {
    let stem = target
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = target
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let parent = target.parent().unwrap_or(Path::new(""));
    (1..)
        .map(|i| parent.join(format!("{stem} ({i}){ext}")))
        .find(|p| !p.exists())
        .unwrap()
}

pub fn file_sha256(p: &Path) -> io::Result<String> {
    let mut f = File::open(p)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; HASH_BUF];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;

    fn random_file(dir: &Path, name: &str, size: usize) -> PathBuf {
        let mut v = vec![0u8; size];
        rand::thread_rng().fill_bytes(&mut v);
        let p = dir.join(name);
        fs::write(&p, v).unwrap();
        p
    }

    fn send_all(
        src: &SourceFile,
        from: u64,
        inc: &mut IncomingFile,
        chunk: usize,
        stop_after: Option<usize>,
    ) -> Option<String> {
        let mut o = OutgoingFile::open(src, from).unwrap();
        let mut buf = Vec::new();
        let mut count = 0;
        while let Some((off, _)) = {
            buf.clear();
            o.next_chunk(&mut buf, chunk).unwrap()
        } {
            inc.write_chunk(off, &buf).unwrap();
            count += 1;
            if Some(count) == stop_after {
                return None;
            }
        }
        Some(o.finish())
    }

    #[test]
    fn full_transfer_with_verification() {
        let s = tempfile::tempdir().unwrap();
        let d = tempfile::tempdir().unwrap();
        let p = random_file(s.path(), "a.bin", 1_000_003);
        let src = &plan(std::slice::from_ref(&p)).unwrap()[0];
        let id = Uuid::new_v4();
        let mut inc = IncomingFile::open(d.path(), &id, 0, &src.offered(), 0).unwrap();
        let sha = send_all(src, 0, &mut inc, 65_000, None).unwrap();
        let (out, digest) = inc.finish(&sha, ConflictPolicy::KeepBoth).unwrap();
        assert_eq!(digest, file_sha256(&p).unwrap());
        assert_eq!(fs::read(out.path()).unwrap(), fs::read(&p).unwrap());
        assert!(
            !d.path().join(PARTIAL_DIR).exists(),
            "dossier partiel nettoyé"
        );
    }

    #[test]
    fn resume_after_interruption() {
        let s = tempfile::tempdir().unwrap();
        let d = tempfile::tempdir().unwrap();
        let p = random_file(s.path(), "big.bin", 3_000_000);
        let src = &plan(std::slice::from_ref(&p)).unwrap()[0];
        let id = Uuid::new_v4();
        let mut inc = IncomingFile::open(d.path(), &id, 0, &src.offered(), 0).unwrap();
        assert!(send_all(src, 0, &mut inc, 65_000, Some(10)).is_none());
        inc.sync().unwrap();
        drop(inc); // coupure réseau
        let offs = resume_offsets(d.path(), &id, &[src.offered()]);
        assert_eq!(offs[0], 650_000);
        let mut inc = IncomingFile::open(d.path(), &id, 0, &src.offered(), offs[0]).unwrap();
        let sha = send_all(src, offs[0], &mut inc, 65_000, None).unwrap();
        let (out, _) = inc.finish(&sha, ConflictPolicy::KeepBoth).unwrap();
        assert_eq!(fs::read(out.path()).unwrap(), fs::read(&p).unwrap());
    }

    #[test]
    fn corruption_detected_and_partial_removed() {
        let s = tempfile::tempdir().unwrap();
        let d = tempfile::tempdir().unwrap();
        let p = random_file(s.path(), "c.bin", 10_000);
        let src = &plan(&[p]).unwrap()[0];
        let id = Uuid::new_v4();
        let mut inc = IncomingFile::open(d.path(), &id, 0, &src.offered(), 0).unwrap();
        send_all(src, 0, &mut inc, 4096, None).unwrap();
        assert!(matches!(
            inc.finish(&"0".repeat(64), ConflictPolicy::KeepBoth),
            Err(TransferError::HashMismatch)
        ));
        assert!(!partial_path(d.path(), &id, 0).exists());
    }

    #[test]
    fn out_of_order_and_overflow_refused() {
        let d = tempfile::tempdir().unwrap();
        let f = OfferedFile {
            rel_path: "x".into(),
            size: 4,
            modified_ms: None,
        };
        let mut inc = IncomingFile::open(d.path(), &Uuid::new_v4(), 0, &f, 0).unwrap();
        assert!(matches!(
            inc.write_chunk(2, b"ab"),
            Err(TransferError::OutOfOrder { .. })
        ));
        assert!(matches!(
            inc.write_chunk(0, b"abcde"),
            Err(TransferError::Overflow)
        ));
        assert!(matches!(
            inc.finish("", ConflictPolicy::Skip),
            Err(TransferError::Incomplete { .. })
        ));
    }

    #[test]
    fn conflicts() {
        let s = tempfile::tempdir().unwrap();
        let d = tempfile::tempdir().unwrap();
        let p = random_file(s.path(), "doc.txt", 100);
        fs::write(d.path().join("doc.txt"), b"autre contenu").unwrap();
        let src = &plan(std::slice::from_ref(&p)).unwrap()[0];
        let run = |policy| {
            let mut inc =
                IncomingFile::open(d.path(), &Uuid::new_v4(), 0, &src.offered(), 0).unwrap();
            let sha = send_all(src, 0, &mut inc, 64, None).unwrap();
            inc.finish(&sha, policy).unwrap().0
        };
        assert_eq!(
            run(ConflictPolicy::KeepBoth),
            FileOutcome::Saved(d.path().join("doc (1).txt"))
        );
        assert_eq!(
            run(ConflictPolicy::Skip),
            FileOutcome::Skipped(d.path().join("doc.txt"))
        );
        assert_eq!(
            fs::read(d.path().join("doc.txt")).unwrap(),
            b"autre contenu"
        );
        assert_eq!(
            run(ConflictPolicy::Replace),
            FileOutcome::Saved(d.path().join("doc.txt"))
        );
        assert_eq!(
            fs::read(d.path().join("doc.txt")).unwrap(),
            fs::read(&p).unwrap()
        );
        // Même contenu déjà présent : pas de doublon.
        assert_eq!(
            run(ConflictPolicy::KeepBoth),
            FileOutcome::AlreadyPresent(d.path().join("doc.txt"))
        );
    }

    #[test]
    fn folder_plan_keeps_tree() {
        let s = tempfile::tempdir().unwrap();
        let root = s.path().join("Photos");
        fs::create_dir_all(root.join("2026/nov")).unwrap();
        random_file(&root, "a.jpg", 10);
        random_file(&root.join("2026/nov"), "b.jpg", 20);
        let v = plan(&[root]).unwrap();
        let rels: Vec<_> = v.iter().map(|f| f.rel_path.as_str()).collect();
        assert_eq!(rels, ["Photos/2026/nov/b.jpg", "Photos/a.jpg"]);
    }

    #[test]
    fn path_sanitization() {
        assert!(safe_relative("../../etc/passwd").is_err());
        assert!(safe_relative("C:/Windows/x").is_err());
        assert!(safe_relative("").is_err());
        assert_eq!(
            safe_relative("/abs/x.txt").unwrap(),
            PathBuf::from("abs").join("x.txt")
        );
        assert_eq!(
            safe_relative("a\\b?.txt").unwrap(),
            PathBuf::from("a").join("b_.txt")
        );
        assert_eq!(safe_relative("CON.txt").unwrap(), PathBuf::from("_CON.txt"));
        assert_eq!(safe_relative("name. ").unwrap(), PathBuf::from("name"));
    }

    #[test]
    fn source_changed_detected() {
        let s = tempfile::tempdir().unwrap();
        let p = random_file(s.path(), "m.bin", 100);
        let mut src = plan(&[p]).unwrap()[0].clone();
        src.size = 50;
        assert!(matches!(
            OutgoingFile::open(&src, 0),
            Err(TransferError::SourceChanged)
        ));
    }

    #[test]
    fn empty_file() {
        let s = tempfile::tempdir().unwrap();
        let d = tempfile::tempdir().unwrap();
        let p = random_file(s.path(), "vide.txt", 0);
        let src = &plan(&[p]).unwrap()[0];
        let mut inc = IncomingFile::open(d.path(), &Uuid::new_v4(), 0, &src.offered(), 0).unwrap();
        let sha = send_all(src, 0, &mut inc, 64, None).unwrap();
        let (out, _) = inc.finish(&sha, ConflictPolicy::KeepBoth).unwrap();
        assert_eq!(fs::metadata(out.path()).unwrap().len(), 0);
    }
}
