//! Searching the BIRD documentation that ships with the BIRD source tree.

use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::error::{ToolError, ToolResult};

const MAX_SECTION_LINES: usize = 200;

#[derive(Debug, Clone)]
pub struct Docs {
    root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hit {
    pub file: String,
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Section {
    pub id: String,
    pub file: String,
    pub start_line: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Chunk {
    pub file: String,
    pub start_line: usize,
    pub end_line: usize,
    pub total_lines: usize,
    pub text: String,
}

impl Docs {
    /// Finds the documentation directory. `configured` wins; otherwise known
    /// checkout layouts are tried.
    pub fn locate(configured: Option<PathBuf>) -> ToolResult<Docs> {
        if let Some(path) = configured {
            let display = path.display().to_string();
            return Docs::at(path).ok_or_else(|| {
                ToolError::Docs(format!(
                    "BIRD_MCP_DOCS_DIR={display} does not contain bird.sgml (point it at the \
                     `doc` directory of a BIRD checkout)"
                ))
            });
        }

        let mut candidates = Vec::new();
        for base in search_roots() {
            candidates.push(base.join("bird/doc"));
            candidates.push(base.join("doc"));
            candidates.push(base.join("bird"));
            candidates.push(base.clone());
        }
        for candidate in candidates {
            if let Some(docs) = Docs::at(candidate) {
                return Ok(docs);
            }
        }

        Err(ToolError::Docs(
            "no BIRD documentation found. Set BIRD_MCP_DOCS_DIR to the `doc` directory of a \
             BIRD checkout (for this workspace: <repo>/bird/doc)."
                .to_string(),
        ))
    }

    fn at(path: PathBuf) -> Option<Docs> {
        for candidate in [path.clone(), path.join("doc")] {
            if candidate.join("bird.sgml").is_file() {
                return Some(Docs { root: candidate });
            }
        }
        None
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return files;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            let wanted = name.ends_with(".sgml")
                || name.ends_with(".md")
                || name == "bird.conf.example"
                || name == "bird.conf.example2";
            if wanted {
                files.push(path);
            }
        }
        files.sort();
        files
    }

    fn display_path(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .display()
            .to_string()
    }

    /// Case-insensitive search. Every whitespace-separated term has to appear on
    /// the line, which keeps multi-word queries from drowning in noise.
    pub fn search(&self, query: &str, limit: usize) -> ToolResult<Vec<Hit>> {
        let terms: Vec<String> = query
            .split_whitespace()
            .map(|term| term.to_lowercase())
            .filter(|term| !term.is_empty())
            .collect();
        if terms.is_empty() {
            return Err(ToolError::BadArgs("query is empty".to_string()));
        }
        let limit = limit.clamp(1, 100);

        let mut hits = Vec::new();
        for path in self.files() {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let file = self.display_path(&path);
            for (index, line) in text.lines().enumerate() {
                let lower = line.to_lowercase();
                if terms.iter().all(|term| lower.contains(term.as_str())) {
                    hits.push(Hit {
                        file: file.clone(),
                        line: index + 1,
                        text: line.trim().chars().take(400).collect(),
                    });
                    if hits.len() >= limit {
                        return Ok(hits);
                    }
                }
            }
        }
        Ok(hits)
    }

    /// Returns the documentation entry that carries `label`, for example
    /// `cli-show-route` or `protocols-bgp`.
    pub fn section(&self, label: &str) -> ToolResult<Section> {
        let needle = format!("<label id=\"{label}\">");
        for path in self.files() {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let lines: Vec<&str> = text.lines().collect();
            let Some(start) = lines.iter().position(|line| line.contains(&needle)) else {
                continue;
            };

            // Include the enclosing chapter/section/tag line when it sits directly
            // above the label, so the returned text carries its title.
            let mut start = start;
            if start > 0 {
                let previous = lines[start - 1].trim_start();
                if previous.starts_with("<tag>")
                    || previous.starts_with("<chapt")
                    || previous.starts_with("<sect")
                    || previous.starts_with("<appendix")
                {
                    start -= 1;
                }
            }

            let mut end = lines.len();
            for (offset, line) in lines.iter().enumerate().skip(start + 1) {
                let trimmed = line.trim_start();
                if trimmed.starts_with("<tag>")
                    || trimmed.starts_with("<sect")
                    || trimmed.starts_with("<chapt")
                    || trimmed.starts_with("<appendix")
                    || trimmed.starts_with("</sect")
                {
                    end = offset;
                    break;
                }
                if offset - start >= MAX_SECTION_LINES {
                    end = offset;
                    break;
                }
            }

            return Ok(Section {
                id: label.to_string(),
                file: self.display_path(&path),
                start_line: start + 1,
                text: lines[start..end].join("\n"),
            });
        }

        Err(ToolError::BadArgs(format!(
            "no documentation label '{label}'. Labels look like cli-show-route, \
             cli-configure-check or protocols-bgp. Use bird_docs_search to find one."
        )))
    }

    /// Reads a line range from one documentation file.
    pub fn read(&self, file: &str, start_line: usize, max_lines: usize) -> ToolResult<Chunk> {
        let path = self.resolve(file)?;
        let text = std::fs::read_to_string(&path)
            .map_err(|err| ToolError::Docs(format!("cannot read {}: {err}", path.display())))?;
        let lines: Vec<&str> = text.lines().collect();
        let start = start_line.clamp(1, lines.len().max(1));
        let count = max_lines.clamp(1, 500);
        let end = (start - 1 + count).min(lines.len());

        Ok(Chunk {
            file: self.display_path(&path),
            start_line: start,
            end_line: end,
            total_lines: lines.len(),
            text: lines[start - 1..end].join("\n"),
        })
    }

    fn resolve(&self, file: &str) -> ToolResult<PathBuf> {
        let relative = Path::new(file);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
        {
            return Err(ToolError::BadArgs(format!(
                "file must be a relative path inside the docs directory: {file}"
            )));
        }
        let path = self.root.join(relative);
        if !path.is_file() {
            return Err(ToolError::BadArgs(format!(
                "{} is not a file in {}",
                file,
                self.root.display()
            )));
        }
        Ok(path)
    }
}

fn search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd.clone());
        if let Some(parent) = cwd.parent() {
            roots.push(parent.to_path_buf());
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut directory = exe.parent().map(Path::to_path_buf);
        for _ in 0..4 {
            let Some(current) = directory else { break };
            roots.push(current.clone());
            directory = current.parent().map(Path::to_path_buf);
        }
    }
    roots
}
