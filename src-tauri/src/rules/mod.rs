//! Built-in detection rules.
//!
//! A rule never matches on a folder name alone: each one carries a `verify`
//! function that inspects the surrounding project (parent directory markers)
//! and/or the candidate's own contents before it is reported.

use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    /// Generated cache that is cheap to recreate.
    Low,
    /// Usually reproducible, but the user should look before removing it.
    Review,
    /// Ambiguous or may contain unique data; needs deliberate attention.
    Caution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Dependencies,
    Environment,
    Cache,
    BuildOutput,
    Checkpoints,
}

/// Context passed to a rule's verifier.
pub struct MatchContext<'a> {
    /// The candidate directory itself.
    pub dir: &'a Path,
    /// Its parent directory (the presumed project root).
    pub parent: &'a Path,
}

pub struct Rule {
    pub id: &'static str,
    pub display_name: &'static str,
    pub ecosystem: &'static str,
    /// Exact directory names this rule applies to (case-sensitive).
    pub match_names: &'static [&'static str],
    pub category: Category,
    pub risk: Risk,
    /// Deletion is always opt-in; kept as data so the UI can explain it.
    pub default_selected: bool,
    pub description: &'static str,
    /// Human-readable description of the context check, shown in the UI.
    pub context_hint: &'static str,
    pub verify: fn(&MatchContext) -> bool,
}

impl std::fmt::Debug for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rule").field("id", &self.id).finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleInfo {
    pub id: &'static str,
    pub display_name: &'static str,
    pub ecosystem: &'static str,
    pub match_names: &'static [&'static str],
    pub category: Category,
    pub risk: Risk,
    pub default_selected: bool,
    pub description: &'static str,
    pub context_hint: &'static str,
}

impl From<&Rule> for RuleInfo {
    fn from(r: &Rule) -> Self {
        RuleInfo {
            id: r.id,
            display_name: r.display_name,
            ecosystem: r.ecosystem,
            match_names: r.match_names,
            category: r.category,
            risk: r.risk,
            default_selected: r.default_selected,
            description: r.description,
            context_hint: r.context_hint,
        }
    }
}

// ---------------------------------------------------------------------------
// Verification helpers
// ---------------------------------------------------------------------------

/// True if `dir/name` exists as a regular file (symlinks are not followed).
fn has_file(dir: &Path, name: &str) -> bool {
    std::fs::symlink_metadata(dir.join(name))
        .map(|m| m.is_file())
        .unwrap_or(false)
}

fn has_entry(dir: &Path, name: &str) -> bool {
    std::fs::symlink_metadata(dir.join(name)).is_ok()
}

fn has_any_file(dir: &Path, names: &[&str]) -> bool {
    names.iter().any(|n| has_file(dir, n))
}

fn has_any_entry(dir: &Path, names: &[&str]) -> bool {
    names.iter().any(|n| has_entry(dir, n))
}

/// Every direct child is a `.pyc`/`.pyo` file (an empty directory also passes).
fn only_bytecode(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries {
        let Ok(entry) = entry else { return false };
        let Ok(ft) = entry.file_type() else { return false };
        if !ft.is_file() {
            return false;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy().to_ascii_lowercase();
        if !(name.ends_with(".pyc") || name.ends_with(".pyo")) {
            return false;
        }
    }
    true
}

const GRADLE_MARKERS: &[&str] = &[
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "settings.gradle.kts",
];

const NEXT_CONFIGS: &[&str] = &[
    "next.config.js",
    "next.config.mjs",
    "next.config.cjs",
    "next.config.ts",
];

const NUXT_CONFIGS: &[&str] = &["nuxt.config.js", "nuxt.config.ts", "nuxt.config.mjs"];

fn verify_node_modules(c: &MatchContext) -> bool {
    has_file(c.parent, "package.json")
}

fn verify_venv(c: &MatchContext) -> bool {
    // A real virtual environment always has pyvenv.cfg at its root.
    has_file(c.dir, "pyvenv.cfg")
}

fn verify_pycache(c: &MatchContext) -> bool {
    only_bytecode(c.dir)
}

fn verify_tool_cache(c: &MatchContext) -> bool {
    // pytest, mypy and ruff all write a CACHEDIR.TAG (or at least a .gitignore)
    // into their cache directory. Requiring one of them avoids matching a
    // hand-made folder that merely shares the name.
    has_any_file(c.dir, &["CACHEDIR.TAG", ".gitignore", "README.md"])
}

fn verify_ipynb_checkpoints(_c: &MatchContext) -> bool {
    true
}

fn verify_next(c: &MatchContext) -> bool {
    has_file(c.parent, "package.json")
        && (has_any_file(c.parent, NEXT_CONFIGS) || has_any_entry(c.dir, &["BUILD_ID", "cache", "server"]))
}

fn verify_nuxt(c: &MatchContext) -> bool {
    has_file(c.parent, "package.json") && has_any_file(c.parent, NUXT_CONFIGS)
}

fn verify_turbo(c: &MatchContext) -> bool {
    has_file(c.parent, "package.json")
        && (has_file(c.parent, "turbo.json") || has_entry(c.dir, "cache"))
}

const BUNDLER_CONFIGS: &[&str] = &[
    "vite.config.js",
    "vite.config.ts",
    "vite.config.mjs",
    "vite.config.cjs",
    "vite.config.mts",
    "webpack.config.js",
    "webpack.config.ts",
    "webpack.config.cjs",
    "webpack.config.mjs",
    "rollup.config.js",
    "rollup.config.ts",
    "rollup.config.mjs",
    "tsup.config.ts",
    "tsup.config.js",
    "angular.json",
    "vue.config.js",
    "svelte.config.js",
    "astro.config.mjs",
    "astro.config.ts",
    ".parcelrc",
];

/// The parent's `.gitignore` lists `name` (as `name`, `/name`, `name/`,
/// `/name/`, `name/*` or `name/**`). Only the first 64 KiB are read.
fn gitignored(parent: &Path, name: &str) -> bool {
    use std::io::Read;
    let Ok(f) = std::fs::File::open(parent.join(".gitignore")) else {
        return false;
    };
    let mut buf = String::new();
    if f.take(64 * 1024).read_to_string(&mut buf).is_err() {
        return false;
    }
    buf.lines().any(|line| {
        let l = line.trim();
        if l.starts_with('#') || l.starts_with('!') {
            return false;
        }
        let l = l.strip_prefix('/').unwrap_or(l);
        let l = l
            .strip_suffix("/**")
            .or_else(|| l.strip_suffix("/*"))
            .or_else(|| l.strip_suffix('/'))
            .unwrap_or(l);
        l == name
    })
}

/// The folder's top level looks like bundler output: emitted JS/CSS, source
/// maps, an index.html or an assets folder.
fn looks_generated(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten().take(500) {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() && matches!(name.as_str(), "assets" | "static" | "chunks" | "js" | "css") {
            return true;
        }
        if ft.is_file()
            && [".js", ".mjs", ".cjs", ".css", ".map", ".html", ".d.ts"]
                .iter()
                .any(|ext| name.ends_with(ext))
        {
            return true;
        }
    }
    false
}

fn verify_js_build_output(c: &MatchContext) -> bool {
    // `dist`/`build` are ambiguous. Require (1) a JS project that is not a
    // Gradle project (handled by its own rule), (2) contents that look like
    // emitted output, and (3) independent evidence that the folder is a
    // build artefact: it is git-ignored, or a bundler config sits next to it.
    let Some(name) = c.dir.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    has_file(c.parent, "package.json")
        && !has_any_file(c.parent, GRADLE_MARKERS)
        && looks_generated(c.dir)
        && (gitignored(c.parent, name) || has_any_file(c.parent, BUNDLER_CONFIGS))
}

fn verify_cargo_target(c: &MatchContext) -> bool {
    has_file(c.parent, "Cargo.toml") && has_any_file(c.dir, &["CACHEDIR.TAG", ".rustc_info.json"])
}

fn verify_maven_target(c: &MatchContext) -> bool {
    has_file(c.parent, "pom.xml")
        && !has_file(c.parent, "Cargo.toml")
        && has_any_entry(c.dir, &["classes", "maven-status", "maven-archiver", "test-classes"])
}

fn verify_gradle_cache(c: &MatchContext) -> bool {
    has_any_file(c.parent, GRADLE_MARKERS)
}

fn verify_gradle_build(c: &MatchContext) -> bool {
    has_any_file(c.parent, GRADLE_MARKERS)
        && has_any_entry(c.dir, &["classes", "libs", "tmp", "intermediates", "generated"])
}

// ---------------------------------------------------------------------------
// Rule table
// ---------------------------------------------------------------------------

pub static RULES: &[Rule] = &[
    Rule {
        id: "node.dependencies",
        display_name: "Node.js dependencies",
        ecosystem: "node",
        match_names: &["node_modules"],
        category: Category::Dependencies,
        risk: Risk::Review,
        default_selected: false,
        description: "Installed npm/pnpm/Yarn packages. Reinstallable from the lockfile.",
        context_hint: "Parent folder contains package.json",
        verify: verify_node_modules,
    },
    Rule {
        id: "python.venv",
        display_name: "Python virtual environment",
        ecosystem: "python",
        match_names: &[".venv", "venv"],
        category: Category::Environment,
        risk: Risk::Review,
        default_selected: false,
        description: "Project virtual environment. May contain packages that were never written to requirements.",
        context_hint: "Folder contains pyvenv.cfg",
        verify: verify_venv,
    },
    Rule {
        id: "python.bytecode",
        display_name: "Python bytecode cache",
        ecosystem: "python",
        match_names: &["__pycache__"],
        category: Category::Cache,
        risk: Risk::Low,
        default_selected: false,
        description: "Compiled .pyc files, regenerated automatically on import.",
        context_hint: "Folder contains only .pyc/.pyo files",
        verify: verify_pycache,
    },
    Rule {
        id: "python.tool-cache",
        display_name: "Python tool cache",
        ecosystem: "python",
        match_names: &[".pytest_cache", ".mypy_cache", ".ruff_cache"],
        category: Category::Cache,
        risk: Risk::Low,
        default_selected: false,
        description: "pytest / mypy / ruff caches, recreated on the next run.",
        context_hint: "Folder contains the tool's cache marker",
        verify: verify_tool_cache,
    },
    Rule {
        id: "jupyter.checkpoints",
        display_name: "Jupyter checkpoints",
        ecosystem: "python",
        match_names: &[".ipynb_checkpoints"],
        category: Category::Checkpoints,
        risk: Risk::Caution,
        default_selected: false,
        description: "Automatic notebook checkpoints. May hold the only recoverable copy of earlier work.",
        context_hint: "Name match only — always inspect before removing",
        verify: verify_ipynb_checkpoints,
    },
    Rule {
        id: "next.build",
        display_name: "Next.js build output",
        ecosystem: "node",
        match_names: &[".next"],
        category: Category::BuildOutput,
        risk: Risk::Review,
        default_selected: false,
        description: "Next.js build output and compiler cache. Rebuilt by `next build` / `next dev`.",
        context_hint: "Parent has package.json and a Next.js config or build marker",
        verify: verify_next,
    },
    Rule {
        id: "nuxt.build",
        display_name: "Nuxt generated output",
        ecosystem: "node",
        match_names: &[".nuxt"],
        category: Category::BuildOutput,
        risk: Risk::Review,
        default_selected: false,
        description: "Generated Nuxt files, recreated by `nuxt dev` / `nuxt build`.",
        context_hint: "Parent has package.json and nuxt.config",
        verify: verify_nuxt,
    },
    Rule {
        id: "turbo.cache",
        display_name: "Turborepo cache",
        ecosystem: "node",
        match_names: &[".turbo"],
        category: Category::Cache,
        risk: Risk::Review,
        default_selected: false,
        description: "Local Turborepo task cache. Removing it makes the next build slower.",
        context_hint: "Parent has package.json and turbo.json (or cache folder present)",
        verify: verify_turbo,
    },
    Rule {
        id: "js.build-output",
        display_name: "Frontend build output",
        ecosystem: "node",
        match_names: &["dist", "build"],
        category: Category::BuildOutput,
        risk: Risk::Caution,
        default_selected: false,
        description: "Common bundler output folder. The name is ambiguous — some projects commit or hand-edit it.",
        context_hint: "Parent has package.json; folder holds emitted JS/CSS/maps; folder is git-ignored or a bundler config is present",
        verify: verify_js_build_output,
    },
    Rule {
        id: "rust.target",
        display_name: "Cargo build artifacts",
        ecosystem: "rust",
        match_names: &["target"],
        category: Category::BuildOutput,
        risk: Risk::Review,
        default_selected: false,
        description: "Cargo compilation output. Rebuilt by `cargo build`, which can take a while.",
        context_hint: "Parent has Cargo.toml; folder has CACHEDIR.TAG or .rustc_info.json",
        verify: verify_cargo_target,
    },
    Rule {
        id: "maven.target",
        display_name: "Maven build output",
        ecosystem: "java",
        match_names: &["target"],
        category: Category::BuildOutput,
        risk: Risk::Review,
        default_selected: false,
        description: "Maven compilation output. Rebuilt by `mvn package`.",
        context_hint: "Parent has pom.xml; folder has classes/ or maven-status/",
        verify: verify_maven_target,
    },
    Rule {
        id: "gradle.project-cache",
        display_name: "Gradle project cache",
        ecosystem: "java",
        match_names: &[".gradle"],
        category: Category::Cache,
        risk: Risk::Caution,
        default_selected: false,
        description: "Per-project Gradle cache. Contents vary by Gradle version.",
        context_hint: "Parent has a build.gradle / settings.gradle file",
        verify: verify_gradle_cache,
    },
    Rule {
        id: "gradle.build",
        display_name: "Gradle build output",
        ecosystem: "java",
        match_names: &["build"],
        category: Category::BuildOutput,
        risk: Risk::Caution,
        default_selected: false,
        description: "Gradle build directory. Scope and contents vary between projects.",
        context_hint: "Parent has a Gradle build file; folder has classes/, libs/ or tmp/",
        verify: verify_gradle_build,
    },
];

/// Directory names the scanner never descends into, even when they do not
/// match a rule. `.git` holds history; `node_modules` is huge and never
/// contains anything we would report separately.
pub const NEVER_DESCEND: &[&str] = &[".git", ".hg", ".svn", "node_modules"];

/// Find the first rule whose name and context checks match `dir`.
pub fn match_dir(dir: &Path) -> Option<&'static Rule> {
    let name = dir.file_name()?.to_str()?;
    let parent = dir.parent()?;
    let ctx = MatchContext { dir, parent };
    RULES
        .iter()
        .filter(|r| r.match_names.contains(&name))
        .find(|r| (r.verify)(&ctx))
}

impl Rule {
    /// Caution items must be ticked one by one and individually acknowledged
    /// at review time; they are never included by "select all".
    pub fn bulk_selectable(&self) -> bool {
        self.risk != Risk::Caution
    }
}

pub fn find_rule(id: &str) -> Option<&'static Rule> {
    RULES.iter().find(|r| r.id == id)
}

/// Re-run a specific rule against a path (used right before cleanup).
pub fn rule_still_matches(rule: &Rule, dir: &Path) -> bool {
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let Some(parent) = dir.parent() else {
        return false;
    };
    rule.match_names.contains(&name) && (rule.verify)(&MatchContext { dir, parent })
}

pub fn all_rules() -> Vec<RuleInfo> {
    RULES.iter().map(RuleInfo::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn touch(p: &Path) {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, b"x").unwrap();
    }

    #[test]
    fn rule_ids_are_unique_and_opt_in() {
        let mut ids: Vec<_> = RULES.iter().map(|r| r.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), RULES.len());
        assert!(RULES.iter().all(|r| !r.default_selected));
    }

    #[test]
    fn node_modules_requires_package_json() {
        let t = tempdir().unwrap();
        let nm = t.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        assert!(match_dir(&nm).is_none());
        touch(&t.path().join("package.json"));
        assert_eq!(match_dir(&nm).unwrap().id, "node.dependencies");
    }

    #[test]
    fn venv_requires_pyvenv_cfg() {
        let t = tempdir().unwrap();
        let v = t.path().join(".venv");
        fs::create_dir(&v).unwrap();
        assert!(match_dir(&v).is_none());
        touch(&v.join("pyvenv.cfg"));
        assert_eq!(match_dir(&v).unwrap().id, "python.venv");
    }

    #[test]
    fn pycache_rejects_non_bytecode() {
        let t = tempdir().unwrap();
        let p = t.path().join("__pycache__");
        touch(&p.join("mod.cpython-312.pyc"));
        assert_eq!(match_dir(&p).unwrap().id, "python.bytecode");
        touch(&p.join("notes.ipynb"));
        assert!(match_dir(&p).is_none());
    }

    #[test]
    fn target_distinguishes_cargo_and_maven() {
        let t = tempdir().unwrap();
        let cargo = t.path().join("rs");
        touch(&cargo.join("Cargo.toml"));
        touch(&cargo.join("target/CACHEDIR.TAG"));
        assert_eq!(match_dir(&cargo.join("target")).unwrap().id, "rust.target");

        let mvn = t.path().join("java");
        touch(&mvn.join("pom.xml"));
        fs::create_dir_all(mvn.join("target/classes")).unwrap();
        assert_eq!(match_dir(&mvn.join("target")).unwrap().id, "maven.target");

        // `target` with no project context is never matched.
        let bare = t.path().join("bare/target");
        fs::create_dir_all(&bare).unwrap();
        assert!(match_dir(&bare).is_none());
    }

    #[test]
    fn ambiguous_build_needs_context() {
        let t = tempdir().unwrap();
        let b = t.path().join("build");
        fs::create_dir(&b).unwrap();
        assert!(match_dir(&b).is_none());
        // package.json alone is not enough any more.
        touch(&t.path().join("package.json"));
        assert!(match_dir(&b).is_none());
        // Emitted output but no evidence it is a build artefact.
        touch(&b.join("index.js"));
        assert!(match_dir(&b).is_none());
        // A bundler config next to it is evidence.
        touch(&t.path().join("vite.config.ts"));
        let rule = match_dir(&b).unwrap();
        assert_eq!(rule.id, "js.build-output");
        assert_eq!(rule.risk, Risk::Caution);
    }

    #[test]
    fn gitignore_is_evidence_for_dist() {
        let t = tempdir().unwrap();
        touch(&t.path().join("package.json"));
        let d = t.path().join("dist");
        touch(&d.join("app.js.map"));
        assert!(match_dir(&d).is_none());
        fs::write(t.path().join(".gitignore"), "node_modules\n/dist/\n").unwrap();
        assert_eq!(match_dir(&d).unwrap().id, "js.build-output");
    }

    #[test]
    fn hand_written_dist_is_not_matched() {
        let t = tempdir().unwrap();
        touch(&t.path().join("package.json"));
        touch(&t.path().join("vite.config.ts"));
        let d = t.path().join("dist");
        // Only documents / data — not bundler output.
        touch(&d.join("report.pdf"));
        touch(&d.join("data.csv"));
        assert!(match_dir(&d).is_none());
    }

    #[test]
    fn only_caution_rules_are_not_bulk_selectable() {
        for r in RULES {
            assert_eq!(r.bulk_selectable(), r.risk != Risk::Caution, "{}", r.id);
        }
        assert_eq!(find_rule("jupyter.checkpoints").unwrap().risk, Risk::Caution);
    }

    #[test]
    fn gradle_build_wins_over_js_build() {
        let t = tempdir().unwrap();
        touch(&t.path().join("package.json"));
        touch(&t.path().join("build.gradle.kts"));
        let b = t.path().join("build");
        fs::create_dir_all(b.join("classes")).unwrap();
        assert_eq!(match_dir(&b).unwrap().id, "gradle.build");
    }

    #[test]
    fn source_like_names_never_match() {
        let t = tempdir().unwrap();
        touch(&t.path().join("package.json"));
        for name in ["src", ".git", ".vscode", ".cursor", "notebooks", ".env"] {
            let d = t.path().join(name);
            fs::create_dir_all(&d).unwrap();
            assert!(match_dir(&d).is_none(), "{name} must not match");
        }
    }
}
