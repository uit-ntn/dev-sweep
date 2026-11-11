<div align="center">

# DevSweep

### Clean your workspace. Reclaim your space.

**A developer-first desktop utility for discovering and cleaning disposable project files, build artifacts, and IDE caches.**

Built with **Tauri 2 · Rust · React · TypeScript**

[Features](#features) · [Supported targets](#supported-cleanup-targets) · [Getting started](#getting-started) · [Safety](#safety-first) · [Roadmap](#roadmap) · [Contributing](#contributing)

</div>

> [!IMPORTANT]
> **Project status: early development (Phase 1 MVP in progress).** The Tauri + React scaffold, the Rust scanner / rule engine / Trash-first cleaner and the four main views are committed, but the app has **not yet been tested end-to-end on Windows** and there is no release. Checked boxes below reflect code that exists in the repository, not a tested release. Keep backups and use on non-critical folders first.

## Overview

Software development leaves behind a surprising amount of **re-creatable data**: `node_modules`, Python virtual environments, compiler output, framework caches, and temporary IDE files. Across many projects these can take up gigabytes of disk space.

**DevSweep** aims to make that disk usage visible and help developers reclaim space **without accidentally deleting source code, notebooks, configuration, or other important data**.

The planned workflow is simple:

1. Choose the directories or drives to inspect.
2. Scan locally for known disposable file and directory patterns.
3. Review the matches, their sizes, locations, and risk classifications.
4. Select exactly what you want to clean.
5. Prefer moving selected items to the OS Trash / Recycle Bin where supported, and display a clear result for each operation.

**No account, cloud upload, or remote scanning should be required for core functionality.**

## Why DevSweep?

| Problem | DevSweep's intended approach |
| --- | --- |
| Old projects each contain huge `node_modules` directories | Find them across explicitly selected roots and report their sizes |
| Python virtual environments accumulate | Identify `.venv` / `venv` folders in project context |
| Build output and caches are difficult to locate | Maintain categorized, extensible cleanup rules |
| IDE cache and user data are easy to confuse | Separate *potentially disposable caches* from settings, extensions, sessions, and workspace state |
| Cleanup tools can be too aggressive | Review-first, conservative defaults, protected paths, and per-item deletion feedback |
| Large scans can freeze a GUI | Background Rust scanning with cancellable operations and incremental progress updates |

## Features

The following items are **planned**, not a statement of currently shipped functionality.

### Core scanning

- [x] Select one or more folders to scan (whole drive roots and the home folder itself are refused for safety).
- [x] Detect common developer-generated directories using built-in rules.
- [x] Show total estimated reclaimable bytes and per-item sizes.
- [x] Filter by ecosystem, path, file type, size, and risk level.
- [x] Sort results by size and inspect a directory before cleanup.
- [x] Stream progress to the interface without blocking the UI.
- [x] Cancel an in-progress scan.
- [x] Exclude chosen directories and protected filesystem locations.
- [x] Avoid unexpectedly following symbolic links, junctions, or mount points.

### Cleanup

- [x] Preview every selected target before any modification.
- [x] Default to **no automatic deletion**.
- [x] Offer OS Trash / Recycle Bin as the preferred cleanup action where supported.
- [ ] Treat permanent deletion as a distinct, clearly warned action, if added.
- [x] Revalidate path, identity, type, and permitted scope immediately before deletion.
- [x] Report successful, skipped, and failed items individually.
- [x] Keep a privacy-conscious local history without storing unnecessary filenames.

### Developer experience

- [ ] Detection rules for JavaScript/TypeScript, Python, Rust, Java, and common web frameworks.
- [ ] Conservative IDE cache inspection for VS Code and Cursor.
- [ ] Custom rules, with safeguards against unsafe broad matches.
- [x] User-maintained allowlist / exclusion list.
- [ ] Windows-first releases, with macOS and Linux under consideration.

## Supported cleanup targets

**Rule classification matters more than folder name.** A matching name alone does not prove that deletion is safe; a detector should consider location, project context, user exclusions, active processes, and whether content is reproducible.

| Ecosystem | Example path or pattern | Expected meaning | Proposed handling |
| --- | --- | --- | --- |
| Node.js / npm / pnpm / Yarn | `node_modules/` | Installed dependencies | Review; generally reinstallable |
| Python | `.venv/`, `venv/` | Project virtual environment | Review; generally recreatable, but may include unrecorded dependencies |
| Python | `__pycache__/`, `*.pyc` | Bytecode cache | Low-risk candidate after inspection |
| Python testing | `.pytest_cache/`, `.mypy_cache/`, `.ruff_cache/` | Tool caches | Low-risk candidate after inspection |
| Jupyter | `.ipynb_checkpoints/` | Automatic notebook checkpoints | **Caution:** may contain recovery versions |
| Next.js | `.next/` | Build output and caches | Review; rebuildable in ordinary projects |
| Nuxt | `.nuxt/` | Generated framework output | Review; usually rebuildable |
| Vite | `node_modules/.vite/` | Dependency pre-bundling cache | Review; regenerable |
| Turborepo | `.turbo/` | Task cache | Review; regenerable, may affect rebuild times |
| Generic frontend | `dist/`, `build/` | Often build output | **Caution:** ambiguous names; require project context |
| Rust | `target/` | Cargo build artifacts | Review; rebuildable; verify Cargo project context |
| Java / Gradle | `.gradle/`, `build/` | Cache or build output | **Caution:** scope and contents vary |
| Java / Maven | `target/` | Common Maven build output | Review; verify Maven project context |
| VS Code | Selected cache/log directories | Editor-generated data | Exact paths and exclusions must be OS/version-aware |
| Cursor | Selected cache/log directories | Editor-generated data | Exact paths and exclusions must be OS/version-aware |

> [!WARNING]
> **Never treat `.ipynb` as junk.** Jupyter `.ipynb` files are notebooks that may contain source code, outputs, and research data. `.ipynb_checkpoints` can also contain recoverable work; it must not be silently removed.

### Explicitly protected data

DevSweep should **not automatically select or delete** the following:

- Source code, Git history (`.git/`), commits, and project assets.
- `.ipynb` notebooks, databases, user-created datasets, documents, and backups.
- `.env`, secrets, SSH keys, access tokens, certificates, or credential stores.
- `.vscode/` and `.cursor/` project settings merely because those folders exist.
- IDE settings, extensions, snippets, account/session databases, workspace storage, and unsaved-editor recovery data.
- Operating-system directories, home-directory roots, or arbitrary user folders.
- Files in use or locations whose ownership and permissions cannot be safely established.

A custom rule must not bypass these protections without deliberate, visible user action. A label such as "cache" is not a guarantee that deleting the item has no side effects.

## Safety first

DevSweep is a **destructive-action utility**: its safety model is a product requirement, not an optional enhancement.

**Proposed safeguards**

1. **Opt-in scope:** scan only roots selected by the user (or otherwise explicitly configured).
2. **Dry-run by default:** display candidates without modifying the filesystem.
3. **No blanket cleanup:** require explicit item selection and a confirmation screen.
4. **Risk levels:** distinguish low-risk generated caches, review-required build outputs, and protected data.
5. **Protected roots:** reject filesystem roots, system directories, sensitive locations, and paths outside authorized scan roots.
6. **Link handling:** do not follow symbolic links, Windows reparse points/junctions, or mount boundaries by default.
7. **Race-condition mitigation:** re-check candidate paths and metadata before acting; avoid deleting a different directory if a path has been replaced since scanning.
8. **Trash first:** use the OS Trash / Recycle Bin when supported; do not imply that recovery is guaranteed.
9. **No silent elevation:** avoid requesting administrator access as a workaround for permission errors.
10. **Transparent outcomes:** show the exact result of every item, including partial failures.

> [!CAUTION]
> Recreating dependencies or build output can require network access, time, storage, original lockfiles, and valid toolchains. Some generated folders can contain unique local modifications. **Always inspect the results and keep backups of important work.**

## Interface

- **Dashboard:** scan roots, estimated reclaimable size, last scan, ecosystem breakdown and aggregate history.
- **Scanner:** live scan progress, match list, ecosystem/risk/size filters, path details, size and risk indicators.
- **Cleanup review:** checked items, estimated size, exact operation, per-item confirmation for *Caution* items, a final confirmation, and per-item results.
- **Disk Analyzer (Windows):** read-only view of a whole drive, with drill-down, largest files and snapshot comparison (see below).
- **Settings:** exclusions, theme, rule catalogue, local history.

### Using the workspace cleaner

1. Add one or more project folders on the Dashboard. Drive roots (`C:\`), your home folder itself and system folders are refused.
2. Add exclusions in Settings if needed. Exclusions are stored by the backend (in the app config folder) and enforced in two places: scans never enter an excluded folder, and **no folder that contains an excluded folder can be cleaned** — e.g. excluding `app/dist/keep` makes `app/dist` *Not cleanable*. Changing exclusions invalidates the current results; scan again before cleaning.
3. Run a scan and select items. **Select all visible** only selects *Low* and *Review* items; *Caution* items (ambiguous `dist`/`build`, `.ipynb_checkpoints`, Gradle folders) must be ticked one by one and confirmed individually on the review screen, otherwise they are skipped.
4. Review and confirm. Items are moved to the Recycle Bin.

**About sizes.** The size shown for an item is the sum of logical file sizes *at scan time* — an estimate. After cleanup DevSweep reports the *estimated size moved to Trash*. It only reports a change in free space when Windows measured one for the drive, and that number includes everything else happening on the drive. Items in the Recycle Bin still use space until it is emptied. Sizes marked `≥` and scans marked *incomplete* hit unreadable entries or were cancelled.

### Disk Analyzer

1. Open **Disk Analyzer**, pick a local drive (C:, D:, …) and click **Analyze**. Progress is shown and the analysis can be cancelled.
2. Browse the largest folders at each level, drill down, and go back with the breadcrumb. The side list shows the largest files in the current folder (taken from the 1,000 largest files on the drive).
3. **Open in Explorer** opens a folder or selects a file. The backend resolves the path from its own analysis tree and re-checks it; the UI never passes a path.
4. **Scan for cleanable items** on a sub-folder hands it to the normal workspace flow (root validation, exclusions, review, confirmation). The Disk Analyzer itself has **no delete action**, and its results cannot be passed to the cleanup command.

The analysis runs with your normal permissions and never asks for administrator rights. Folders it cannot read are skipped and counted, and the result is marked incomplete. Symlinks, junctions, other reparse points and volumes mounted into folders are not followed. Hard-linked files are counted once per link.

### Snapshots and growth over time

After each **completed** analysis (not cancelled) a snapshot is saved locally in the app data folder: volume serial and root, time, free space reported by Windows, the size of every folder at most two levels deep plus every folder of 32 MiB or more (up to 20,000), and the list of unreadable folders. The newest 30 snapshots are kept, and you can delete one or all of them on the **Snapshots & growth** tab.

Pick two snapshots of the **same drive** to compare. You see:

- the change in **used space measured by Windows** and the real time between the two scans (e.g. *Grew 12 GB in 2 days*);
- separately, the change in **logical size of readable folders**, and the folders that grew or shrank the most.

A comparison is marked **partial** when either scan had unreadable folders, the unreadable set differs, a folder limit was hit, or the scans used different detail settings. Folders inside an unreadable area are left out rather than shown as deleted. Snapshots of different drives cannot be compared. Scans are manual only: there is no scheduler, no telemetry and nothing leaves the machine.

### Known limitations

- The Rust backend and the Disk Analyzer have not yet been tested end-to-end on a real Windows machine.
- Drive listing and free-space measurement are Windows-only; on other platforms the Disk Analyzer shows no drives.
- The analyzer keeps the folder tree in memory (capped at 5 million folders); very large drives use a few hundred MB of RAM while browsing.
- Sizes are logical sizes, not allocated size on disk (compression, sparse files and cluster slack are not accounted for).
- Directory identity on Windows uses the NTFS file index; on file systems without stable file ids it falls back to creation time.

*Screenshots and demos will be added once a working UI exists.*

## Tech stack

| Layer | Proposed technology | Responsibility |
| --- | --- | --- |
| Desktop runtime | [Tauri 2](https://tauri.app/) | Native app window, IPC, packaging, platform integration |
| Scanner & cleanup backend | [Rust](https://www.rust-lang.org/) | Filesystem enumeration, size calculation, validation, cleanup |
| UI | [React](https://react.dev/) | Responsive desktop interface |
| Language | [TypeScript](https://www.typescriptlang.org/) | Typed frontend application logic |
| Bundler | [Vite](https://vite.dev/) | Frontend development and builds |
| Styling | [Tailwind CSS](https://tailwindcss.com/) | UI styling |
| UI components | [shadcn/ui](https://ui.shadcn.com/) | Accessible reusable components |
| Client state | [Zustand](https://zustand.docs.pmnd.rs/) (optional) | Scan state, filters, selection, preferences |
| Rust crates | `ignore` / `walkdir`, `serde`, `trash`, optionally `rayon` | Candidate discovery, data transfer, Trash handling, parallel work |

These are **proposed dependencies**; the final dependency list should match the repository's real manifests. Parallelism and filesystem traversal must be designed carefully to avoid excessive disk I/O.

## Architecture

```text
┌─────────────────────────────────────────────────────┐
│                   React + TypeScript                │
│  Dashboard · Scanner · Review · Preferences         │
└───────────────────────┬─────────────────────────────┘
                        │ Tauri commands + events
┌───────────────────────▼─────────────────────────────┐
│                  Tauri / Rust Core                  │
│                                                     │
│  Scan coordinator                                   │
│   ├── Directory traversal and exclusions            │
│   ├── Rule matching and project-context checks      │
│   ├── Size estimation and progress reporting        │
│   └── Cancellation and result collection            │
│                                                     │
│  Cleanup coordinator                                │
│   ├── Path and scope revalidation                   │
│   ├── User approval / risk checks                   │
│   ├── Trash / Recycle Bin operations                │
│   └── Error and audit reporting                     │
└───────────────────────┬─────────────────────────────┘
                        │
                Local filesystem only
```

### Suggested directory layout

The following is a **target structure**, not a claim that all these files already exist:

```text
dev-sweep/
├── README.md
├── LICENSE
├── .gitignore
├── package.json
├── src/
│   ├── components/
│   ├── pages/
│   │   ├── Dashboard.tsx
│   │   ├── Scanner.tsx
│   │   ├── CleanupReview.tsx
│   │   └── Settings.tsx
│   ├── hooks/
│   ├── stores/
│   └── types/
└── src-tauri/
    ├── tauri.conf.json
    ├── Cargo.toml
    └── src/
        ├── lib.rs
        ├── commands.rs
        ├── scanner/
        ├── rules/
        └── cleaner/
```

## Getting started

> [!NOTE]
> The repository is already scaffolded with **Tauri 2 + React + TypeScript + npm**; you do not need to run `create-tauri-app`.

### Prerequisites (Windows development)

Install:

1. **Node.js** (an actively supported LTS version) and npm.
2. **Rust** using [rustup](https://rustup.rs/) with the MSVC toolchain.
3. **Microsoft C++ Build Tools**, including the *Desktop development with C++* workload.
4. **Microsoft Edge WebView2 Runtime** (present on many current Windows systems, but required to run the UI).
5. **Git**.

See Tauri's official [Windows prerequisites](https://v2.tauri.app/start/prerequisites/) for the current toolchain and operating-system requirements.

### Create the Tauri scaffold (initial maintainers only)

If the repository currently contains only documentation, scaffold the app using:

```bash
npm create tauri-app@latest
```

Choose a project name such as `dev-sweep`, **TypeScript**, **React**, and **npm** when prompted. Move or initialize the scaffold in the intended repository and merge any pre-existing `README.md`, `.gitignore`, and `LICENSE` carefully. Do **not** overwrite repository history or documentation unintentionally.

### Run a scaffolded repository

```bash
git clone https://github.com/uit-ntn/dev-sweep.git
cd dev-sweep
npm install
npm run tauri dev
```

Run the tests (Rust tests use temporary-directory fixtures and a fake remover — they never touch the real Trash or your drives):

```bash
npm run typecheck
npm test                 # frontend store tests (Vitest)
cd src-tauri
cargo test
cargo clippy --all-targets
```

### Project layout

```text
src/                     React UI
  pages/                 Dashboard, Scanner, CleanupReview, DiskAnalyzer, Settings
  stores/useSweepStore   Workspace state (roots, scan, filters, selection, history)
  stores/useDiskStore    Disk Analyzer state (volumes, analysis, snapshots)
  lib/api.ts             Typed wrappers around Tauri commands
src-tauri/src/
  rules/                 Built-in detection rules + project-context checks
  scanner/               Cancellable traversal, sizing, progress events
  cleaner/               Revalidation + Trash-first cleanup
  workspace.rs           Scan sessions and scan/cleanup locking
  exclusions.rs          Backend-owned, persisted exclusion policy
  disk_usage.rs          Read-only drive analysis, volumes, Explorer reveal
  snapshots.rs           Local snapshots and comparisons
  safety.rs              Protected paths, link/junction detection, path keys, dir identity
  commands.rs            Tauri commands for both flows
```

**Safety design in the code:**

- The UI never sends a path to delete — only ids of the form `<session>:c<n>` from the current scan session. A cleanup naming an older session is rejected, and results only update the session that owns them.
- Scans and cleanups are serialized in the backend (`workspace.rs`); a conflicting request is rejected even if the UI has not caught up.
- Before each item is moved to the Trash the backend re-checks that it still exists, is not a link/junction, has the same identity (NTFS file index / inode + birth time), resolves to the same canonical path, sits strictly inside its scan root, is not protected, does not contain or sit inside an excluded folder (component-wise, case-insensitive on Windows), still matches its rule, and — for *Caution* items — was individually confirmed. Anything that fails is reported as *Skipped*.
- Disk Analyzer commands accept only a drive returned by `list_volumes` and return numeric node indices that the cleanup command does not accept.

### Frontend-only development (when applicable)

```bash
npm run dev
```

This may preview the frontend, but filesystem scanning and native cleanup features require the Tauri host; a regular browser cannot access arbitrary local folders.

## Build for Windows

On a properly configured **Windows** build machine:

```bash
npm install
npm run tauri build
```

Typical output paths after a successful release build:

```text
src-tauri/target/release/dev-sweep.exe
src-tauri/target/release/bundle/nsis/*-setup.exe
src-tauri/target/release/bundle/msi/*.msi
```

The exact binary name and bundle types depend on `src-tauri/tauri.conf.json`, the crate configuration, build targets, and bundler availability. Not every build produces both NSIS and MSI artifacts.

**Installer versus portable executable**

- **NSIS `*-setup.exe`:** installer that installs DevSweep on Windows.
- **MSI `.msi`:** Windows Installer package for deployment.
- **Raw `target/release/*.exe`:** compiled executable; it is **not automatically a complete tested portable distribution**. Required runtime components and assets still matter.

The application does not require Node.js or a Rust toolchain on the end-user's machine, but it does require the appropriate Windows WebView2 runtime. Unsigned binaries may trigger operating-system reputation warnings; official releases should eventually use code signing.

For current bundling details, consult [Tauri's Windows installer documentation](https://v2.tauri.app/distribute/windows-installer/).

## How matching rules should work

A future detection rule should include more than a folder name:

| Field | Example | Purpose |
| --- | --- | --- |
| `id` | `node.dependencies` | Stable rule identifier |
| `display_name` | `Node.js dependencies` | User-friendly result label |
| `match_name` | `node_modules` | Candidate directory name |
| `project_marker` | `package.json` | Verify surrounding project context |
| `category` | `dependencies` | Group related results |
| `risk` | `review` | Suggested handling |
| `default_selected` | `false` | Keep deletion opt-in |

Illustrative rule **concept** (not a shipped config format):

```json
{
  "id": "node.dependencies",
  "display_name": "Node.js dependencies",
  "match_name": "node_modules",
  "project_marker": "package.json",
  "category": "dependencies",
  "risk": "review",
  "default_selected": false
}
```

For ambiguous names such as `build`, `dist`, or `target`, require ecosystem markers or a stronger detector. Implement strict validation for user-defined rules; never execute arbitrary shell commands from a rule file.

## Performance considerations

- Use asynchronous/background execution so the UI remains responsive.
- Report progress incrementally rather than sending millions of individual messages to React.
- Limit parallel reads to avoid saturating slow disks or network shares.
- Handle unreadable folders, deeply nested paths, Windows long paths, and non-UTF-8 filenames gracefully.
- Keep file sizes in integer bytes internally; format KiB/MiB/GiB only for display.
- Treat scan size as an **estimate**; actual freed space may differ due to hard links, compression, sparse files, Trash behavior, and concurrent modifications.
- Avoid scanning network drives or external volumes unless the user opts in.

## Privacy and permissions

DevSweep's intended core scan and cleanup workflow runs **locally**. It should not upload file names, directory paths, file contents, or scan results to a server.

Any future update checks, telemetry, or crash reporting should be **documented separately**, disabled by default unless explicitly justified, and configurable. Logging should avoid secrets and sensitive path contents. DevSweep should request only filesystem access needed to perform actions selected by the user.

## Roadmap

### Phase 1 — Minimum viable product

- [x] Initialize Tauri 2 + React + TypeScript project.
- [x] Create the main desktop window and navigation.
- [x] Implement user-selected scan roots.
- [x] Detect `node_modules`, `.venv`, `__pycache__`, `.next`, and Rust `target` with context checks.
- [x] Calculate directory sizes and display results.
- [x] Implement safe preview, explicit selection, and Trash-first cleanup.
- [x] Add cancellation and error handling.
- [ ] End-to-end testing on Windows.

### Phase 2 — Better developer coverage

- [x] Add framework/test/build cache rules (Nuxt, Turborepo, pytest/mypy/ruff, Maven, Gradle, `dist`/`build`).
- [ ] Vite cache (`node_modules/.vite`) as a separate item — currently counted as part of `node_modules`.
- [ ] Add conservative VS Code and Cursor cache inspection.
- [ ] Add exclusions and custom scan profiles.
- [ ] Improve progress, sorting, and storage visualization.
- [ ] Add automated tests for safety invariants and edge cases (initial Rust unit tests exist; more needed).

### Phase 3 — Distribution and polish

- [ ] Publish signed or clearly labeled Windows releases.
- [ ] Add a documented portable packaging process, if feasible.
- [ ] Improve accessibility, localization, and theme support.
- [ ] Evaluate macOS and Linux support and their Trash semantics.
- [ ] Introduce safe update checks and release automation.

## Testing strategy

Cleanup code should be tested more rigorously than ordinary UI code. The proposed test suite should cover:

- Project marker detection and false positives.
- Nested or overlapping matches (avoid double-counting and double deletion).
- Symlinks, junctions, reparse points, and path traversal.
- Root-directory protection and restricted user permissions.
- Renamed, moved, or replaced targets between scan and cleanup.
- Concurrent scans, cancellation, and partial filesystem failures.
- Empty directories, huge trees, unusual filenames, and locked files.
- Preview/selection consistency and non-destructive default behavior.

Prefer disposable temporary directories and synthetic fixtures for automated tests. **Never point destructive tests at a developer's real home directory or active projects.**

## FAQ

<details>
<summary><strong>Does DevSweep delete my source code?</strong></summary>

It is designed **not to**. The planned behavior is conservative: restrict scan roots, protect sensitive locations, classify risk, show a preview, and require explicit selection. Until these safeguards are implemented and tested, do not use experimental builds on important data without backups.

</details>

<details>
<summary><strong>Is deleting node_modules or .venv always safe?</strong></summary>

No. These directories are usually reproducible, but local modifications, undocumented dependencies, editable installs, missing lockfiles, or unavailable packages may make restoration difficult. Inspect the project first.

</details>

<details>
<summary><strong>Will deleting IDE cache remove my settings?</strong></summary>

It should not. IDE settings, extension data, workspaces, sessions, and recovery data must be treated separately from verified disposable caches. Exact editor paths vary by OS, version, and profile.

</details>

<details>
<summary><strong>Are Jupyter notebooks considered junk?</strong></summary>

**No.** `.ipynb` files are important user documents. Even `.ipynb_checkpoints` may hold useful recovery data and should require special care.

</details>

<details>
<summary><strong>Can I undo cleanup?</strong></summary>

Moving items to Trash / Recycle Bin may make restoration possible, but this varies across operating systems and storage devices. Permanent deletion, if offered, cannot be assumed reversible. Undo support should never be advertised unless actually implemented.

</details>

<details>
<summary><strong>Is DevSweep available for macOS and Linux?</strong></summary>

The initial target is **Windows**. Other desktop platforms are longer-term goals, not confirmed releases.

</details>

<details>
<summary><strong>Does DevSweep need an internet connection?</strong></summary>

The **planned core functionality** is local-only. Installing dependencies, updating the application, or recreating deleted project dependencies may require a connection.

</details>

## Contributing

Contributions, bug reports, and ideas are welcome once the implementation and contribution process are in place.

1. Check existing Issues and discussions to avoid duplicates.
2. Open an Issue describing a proposed feature, detection rule, or bug.
3. For nontrivial changes, discuss the design before implementing it.
4. Make changes on a feature branch and add tests where appropriate.
5. Explain safety implications for any filesystem-affecting change.
6. Submit a pull request with reproduction steps and screenshots if relevant.

**Please do not include private directory listings, secrets, or user files in bug reports.** Redact sensitive paths and data.

### Example contribution areas

- Add a context-aware cleanup detector for a new ecosystem.
- Improve Windows path and junction handling.
- Help design accessible scan and review UI.
- Write tests for deletion safeguards and rule matching.
- Translate interface strings and documentation.

## Security

If you discover a vulnerability that could delete unintended files, bypass protected paths, expose user data, or execute unsafe commands, **avoid posting an exploit publicly**. Contact the maintainers privately through any security reporting channel they publish. A dedicated `SECURITY.md` and private vulnerability reporting workflow are planned.

## License

**MIT** is the intended license for this project **if an MIT `LICENSE` file is actually committed**. The repository's `LICENSE` file, once present, is authoritative. Until a license is included, do not assume the code has been granted an open-source license.

## Acknowledgements

DevSweep builds on the ecosystem of [Tauri](https://tauri.app/), [Rust](https://www.rust-lang.org/), [React](https://react.dev/), and the open-source developer tooling community.

---

<div align="center">

**DevSweep** — *Clean your workspace. Reclaim your space.*

Made for developers who would rather ship code than hunt down gigabytes of old build output.

</div>
