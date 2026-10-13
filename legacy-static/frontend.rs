//! Parser frontend: sv-parser + sv-parser-pp with a prelude-define pass and
//! include paths, so that macro-heavy RTL (OpenTitan) parses without needing
//! a full build-system elaboration.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use sv_parser::{parse_sv, preprocess, Defines, SyntaxTree};

pub struct Frontend {
    pub includes: Vec<PathBuf>,
    pub defines: Defines,
    pub prelude_count: usize,
    pub prelude_errors: Vec<String>,
}

impl Frontend {
    /// Build a frontend seeded with macro definitions accumulated from a set of
    /// prelude files (e.g. prim_assert.sv). Macros defined there are used by
    /// other files without being included by them.
    pub fn new(includes: Vec<PathBuf>, preludes: &[PathBuf]) -> Self {
        let empty: Defines = HashMap::new();
        let mut defines: Defines = HashMap::new();
        let mut errors = Vec::new();
        let mut count = 0usize;
        for p in preludes {
            match preprocess(p, &empty, &includes, false, false) {
                Ok((_t, d)) => {
                    for (k, v) in d {
                        defines.insert(k, v);
                    }
                    count += 1;
                }
                Err(e) => errors.push(format!("{}: {:?}", p.display(), e)),
            }
        }
        Frontend {
            includes,
            defines,
            prelude_count: count,
            prelude_errors: errors,
        }
    }

    /// Parse one file. Returns the syntax tree, or the error rendered as text.
    pub fn parse(&self, path: &Path) -> Result<SyntaxTree, String> {
        parse_sv(path, &self.defines, &self.includes, false, false)
            .map(|(t, _)| t)
            .map_err(|e| format!("{:?}", e))
    }
}
