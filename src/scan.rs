//! Varredura recursiva das pastas: coleta caminho e tamanho de cada arquivo comum.

use std::fmt;
use std::path::PathBuf;

use walkdir::{DirEntry, WalkDir};

/// Filtros aplicados durante a varredura.
#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Arquivos menores que isto (em bytes) são ignorados. O padrão da CLI é 1,
    /// para que arquivos vazios não virem um grupo gigante de "duplicatas".
    pub min_size: u64,
    /// Pula arquivos e pastas ocultos (nome começando com `.` e, no Windows,
    /// também o atributo "oculto").
    pub ignore_hidden: bool,
}

/// Um arquivo candidato a duplicata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: u64,
}

/// Falha de leitura não fatal: o item é descartado e a busca continua.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanError {
    pub path: Option<PathBuf>,
    pub message: String,
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.path {
            Some(path) => write!(f, "{}: {}", path.display(), self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for ScanError {}

impl From<walkdir::Error> for ScanError {
    fn from(err: walkdir::Error) -> Self {
        let path = err.path().map(PathBuf::from);
        // A mensagem do io::Error já basta; o caminho vai no campo próprio.
        let message = match err.io_error() {
            Some(io) => io.to_string(),
            None => err.to_string(),
        };
        Self { path, message }
    }
}

/// Saída da varredura.
#[derive(Debug, Default)]
pub struct ScanOutcome {
    pub files: Vec<FileEntry>,
    pub errors: Vec<ScanError>,
}

/// Percorre cada raiz sem seguir links simbólicos (evita ciclos e evita contar
/// o mesmo arquivo duas vezes pelo link).
pub fn scan(roots: &[PathBuf], options: &ScanOptions) -> ScanOutcome {
    let mut outcome = ScanOutcome::default();

    for root in disjoint_roots(roots, &mut outcome.errors) {
        let walker = WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            // A raiz (profundidade 0) foi pedida explicitamente: nunca é filtrada.
            .filter_entry(|entry| {
                !(options.ignore_hidden && entry.depth() > 0 && is_hidden(entry))
            });

        for entry in walker {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    outcome.errors.push(err.into());
                    continue;
                }
            };
            if !entry.file_type().is_file() {
                continue;
            }
            match entry.metadata() {
                Ok(metadata) if metadata.len() >= options.min_size => {
                    outcome.files.push(FileEntry {
                        size: metadata.len(),
                        path: entry.into_path(),
                    });
                }
                Ok(_) => {}
                Err(err) => outcome.errors.push(err.into()),
            }
        }
    }

    outcome
}

/// Remove raízes repetidas ou contidas em outra (`dupfind . ./src`), para que
/// nenhum arquivo seja visitado duas vezes e apareça como duplicata de si mesmo.
/// A comparação usa o caminho canônico; a saída mantém o caminho como foi digitado.
fn disjoint_roots(roots: &[PathBuf], errors: &mut Vec<ScanError>) -> Vec<PathBuf> {
    let mut resolved: Vec<(PathBuf, &PathBuf)> = Vec::with_capacity(roots.len());
    for root in roots {
        match root.canonicalize() {
            Ok(canonical) => resolved.push((canonical, root)),
            Err(err) => errors.push(ScanError {
                path: Some(root.clone()),
                message: err.to_string(),
            }),
        }
    }

    // Ordenado por componentes, o pai sempre vem antes dos filhos.
    resolved.sort_by(|a, b| a.0.cmp(&b.0));

    let mut kept: Vec<(PathBuf, &PathBuf)> = Vec::with_capacity(resolved.len());
    for (canonical, original) in resolved {
        if !kept.iter().any(|(parent, _)| canonical.starts_with(parent)) {
            kept.push((canonical, original));
        }
    }
    kept.into_iter()
        .map(|(_, original)| original.clone())
        .collect()
}

fn is_hidden(entry: &DirEntry) -> bool {
    if entry.file_name().as_encoded_bytes().starts_with(b".") {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        // No Windows o walkdir já traz os metadados da listagem: não custa uma chamada a mais.
        if let Ok(metadata) = entry.metadata()
            && metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn nested_and_repeated_roots_are_collapsed() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();

        let roots = vec![sub, dir.path().to_path_buf(), dir.path().to_path_buf()];
        let mut errors = Vec::new();

        assert_eq!(
            disjoint_roots(&roots, &mut errors),
            vec![dir.path().to_path_buf()]
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn sibling_roots_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let ab = dir.path().join("ab");
        fs::create_dir(&a).unwrap();
        fs::create_dir(&ab).unwrap();

        // `ab` começa com o texto `a`, mas não está dentro da pasta `a`.
        let mut kept = disjoint_roots(&[a.clone(), ab.clone()], &mut Vec::new());
        kept.sort();
        assert_eq!(kept, vec![a, ab]);
    }

    #[test]
    fn missing_root_is_reported_as_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing");
        let mut errors = Vec::new();

        assert!(disjoint_roots(&[missing.clone()], &mut errors).is_empty());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path.as_ref(), Some(&missing));
    }
}
