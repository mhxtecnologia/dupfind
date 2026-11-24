//! `dupfind` localiza arquivos duplicados em três etapas, da mais barata para a
//! mais cara: agrupa por tamanho (sem abrir nenhum arquivo), compara o BLAKE3
//! dos primeiros 16 KiB e só então calcula o hash completo dos que sobraram.
//!
//! A biblioteca não imprime nada: devolve um [`Report`] que a CLI formata.

pub mod dedup;
pub mod report;
pub mod scan;
pub mod size;

use std::path::PathBuf;

pub use dedup::DuplicateGroup;
pub use scan::{ScanError, ScanOptions};

/// Resultado completo de uma busca.
#[derive(Debug, Default)]
pub struct Report {
    /// Grupos de arquivos com conteúdo idêntico, do que mais libera espaço ao que menos libera.
    pub groups: Vec<DuplicateGroup>,
    /// Arquivos ou pastas que não puderam ser lidos; a busca segue sem eles.
    pub errors: Vec<ScanError>,
    /// Quantos arquivos passaram pelos filtros da varredura.
    pub scanned_files: usize,
}

/// Números agregados do relatório, usados no rodapé do texto e no JSON.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Summary {
    pub scanned_files: usize,
    pub duplicate_groups: usize,
    pub redundant_files: usize,
    pub reclaimable_bytes: u64,
    pub errors: usize,
}

impl Report {
    pub fn summary(&self) -> Summary {
        Summary {
            scanned_files: self.scanned_files,
            duplicate_groups: self.groups.len(),
            redundant_files: self.groups.iter().map(|g| g.paths.len() - 1).sum(),
            reclaimable_bytes: self
                .groups
                .iter()
                .map(DuplicateGroup::reclaimable_bytes)
                .sum(),
            errors: self.errors.len(),
        }
    }
}

/// Varre as raízes e devolve os grupos de duplicatas encontrados.
pub fn find_duplicates(roots: &[PathBuf], options: &ScanOptions) -> Report {
    let scanned = scan::scan(roots, options);
    let scanned_files = scanned.files.len();
    let dedup = dedup::group_duplicates(scanned.files);

    let mut errors = scanned.errors;
    errors.extend(dedup.errors);

    Report {
        groups: dedup.groups,
        errors,
        scanned_files,
    }
}
