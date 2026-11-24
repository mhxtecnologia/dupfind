//! Agrupamento em etapas: tamanho → hash do prefixo → hash completo.
//!
//! Cada etapa só recebe os arquivos que ainda colidem na anterior, então a
//! maior parte do disco nunca é lida: arquivos de tamanho único são
//! descartados sem abrir, e arquivos grandes que diferem logo no começo param
//! nos primeiros 16 KiB.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::scan::{FileEntry, ScanError};

/// Quantos bytes do início de cada arquivo entram no hash parcial.
pub const PREFIX_BYTES: u64 = 16 * 1024;

/// Arquivos com o mesmo tamanho e o mesmo BLAKE3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateGroup {
    pub size: u64,
    pub hash: blake3::Hash,
    /// Ordenados, para a saída ser determinística.
    pub paths: Vec<PathBuf>,
}

impl DuplicateGroup {
    /// Espaço liberado ao manter uma única cópia.
    pub fn reclaimable_bytes(&self) -> u64 {
        self.size * (self.paths.len() as u64).saturating_sub(1)
    }
}

#[derive(Debug, Default)]
pub struct DedupOutcome {
    pub groups: Vec<DuplicateGroup>,
    pub errors: Vec<ScanError>,
}

pub fn group_duplicates(files: Vec<FileEntry>) -> DedupOutcome {
    let mut errors = Vec::new();

    // Etapa 1: tamanho. Só metadados, nenhum arquivo é aberto.
    let mut by_size: HashMap<u64, Vec<FileEntry>> = HashMap::new();
    for file in files {
        by_size.entry(file.size).or_default().push(file);
    }
    let same_size: Vec<FileEntry> = by_size
        .into_values()
        .filter(|group| group.len() > 1)
        .flatten()
        .collect();

    // Etapa 2: prefixo. Para arquivo de até PREFIX_BYTES o prefixo é o arquivo
    // inteiro, então o hash dele já é o definitivo.
    let mut groups = Vec::new();
    let mut needs_full_hash = Vec::new();
    for (size, hash, files) in hash_and_group(same_size, Some(PREFIX_BYTES), &mut errors) {
        if size <= PREFIX_BYTES {
            groups.push(into_group(size, hash, files));
        } else {
            needs_full_hash.extend(files);
        }
    }

    // Etapa 3: hash completo, só para os arquivos grandes que empataram no prefixo.
    for (size, hash, files) in hash_and_group(needs_full_hash, None, &mut errors) {
        groups.push(into_group(size, hash, files));
    }

    groups.sort_by(|a, b| {
        b.reclaimable_bytes()
            .cmp(&a.reclaimable_bytes())
            .then_with(|| a.paths.cmp(&b.paths))
    });
    errors.sort_by(|a, b| a.path.cmp(&b.path));

    DedupOutcome { groups, errors }
}

type HashedGroup = (u64, blake3::Hash, Vec<FileEntry>);

/// Calcula o hash de cada arquivo em paralelo (rayon) e devolve só os grupos
/// com duas ou mais entradas. A chave inclui o tamanho: dois arquivos de tamanhos
/// diferentes nunca se juntam, mesmo com o prefixo igual.
fn hash_and_group(
    files: Vec<FileEntry>,
    limit: Option<u64>,
    errors: &mut Vec<ScanError>,
) -> Vec<HashedGroup> {
    let hashed: Vec<(FileEntry, io::Result<blake3::Hash>)> = files
        .into_par_iter()
        .map(|file| {
            let result = hash_file(&file.path, limit);
            (file, result)
        })
        .collect();

    let mut buckets: HashMap<(u64, blake3::Hash), Vec<FileEntry>> = HashMap::new();
    for (file, result) in hashed {
        match result {
            Ok(hash) => buckets.entry((file.size, hash)).or_default().push(file),
            Err(err) => errors.push(ScanError {
                path: Some(file.path),
                message: err.to_string(),
            }),
        }
    }

    buckets
        .into_iter()
        .filter(|(_, files)| files.len() > 1)
        .map(|((size, hash), files)| (size, hash, files))
        .collect()
}

/// BLAKE3 do arquivo inteiro, ou só dos primeiros `limit` bytes. O
/// `update_reader` lê em blocos internos de 64 KiB: memória constante, seja
/// qual for o tamanho do arquivo.
fn hash_file(path: &Path, limit: Option<u64>) -> io::Result<blake3::Hash> {
    let file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    match limit {
        Some(limit) => {
            hasher.update_reader(file.take(limit))?;
        }
        None => {
            hasher.update_reader(file)?;
        }
    }
    Ok(hasher.finalize())
}

fn into_group(size: u64, hash: blake3::Hash, files: Vec<FileEntry>) -> DuplicateGroup {
    let mut paths: Vec<PathBuf> = files.into_iter().map(|file| file.path).collect();
    paths.sort();
    DuplicateGroup { size, hash, paths }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reclaimable_bytes_counts_every_copy_but_one() {
        let group = DuplicateGroup {
            size: 100,
            hash: blake3::hash(b"x"),
            paths: vec!["a".into(), "b".into(), "c".into()],
        };
        assert_eq!(group.reclaimable_bytes(), 200);
    }

    #[test]
    fn prefix_hash_matches_full_hash_for_small_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("small.bin");
        std::fs::write(&path, b"short content").unwrap();

        let prefix = hash_file(&path, Some(PREFIX_BYTES)).unwrap();
        assert_eq!(prefix, blake3::hash(b"short content"));
        assert_eq!(prefix, hash_file(&path, None).unwrap());
    }
}
