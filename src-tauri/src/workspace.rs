//! Workspace scan/cleanup state machine.
//!
//! Kept free of Tauri types so the concurrency and session rules can be unit
//! tested. The command layer holds a `Mutex<Workspace>`, calls `begin_*`
//! under the lock, does the slow filesystem work *outside* the lock, then
//! calls `finish_*` (or `abort`) under the lock again.
//!
//! Rules enforced here (not in the UI):
//! * only one scan or cleanup at a time — conflicting requests are rejected;
//! * a cleanup must name the current session; ids from an older session are
//!   rejected wholesale;
//! * cleanup results only touch the session that owns the items;
//! * changing exclusions after a scan marks that scan stale, so its results
//!   can no longer be cleaned until the user scans again.

use crate::cleaner::{CleanItemResult, CleanStatus};
use crate::exclusions::ExclusionPolicy;
use crate::scanner::{CandidateRecord, ScanOutput};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Busy {
    Idle,
    Scanning,
    Cleaning,
}

#[derive(Debug)]
pub struct Session {
    pub id: String,
    records: HashMap<String, CandidateRecord>,
    sizes: HashMap<String, u64>,
    policy_fingerprint: Vec<Vec<String>>,
    stale: Option<String>,
}

/// A started scan. Must be passed back to `finish_scan` or `abort`.
#[derive(Debug)]
pub struct ScanTicket {
    pub session_id: String,
    pub policy: ExclusionPolicy,
}

/// A started cleanup: an owned copy of what the cleaner needs.
#[derive(Debug)]
pub struct CleanJob {
    pub session_id: String,
    pub ids: Vec<String>,
    pub records: HashMap<String, CandidateRecord>,
    pub sizes: HashMap<String, u64>,
    pub policy: ExclusionPolicy,
    pub acknowledged: HashSet<String>,
}

#[derive(Debug)]
pub struct Workspace {
    busy: Busy,
    session: Option<Session>,
    counter: u64,
    /// Random-ish per-process salt so ids never repeat across app restarts.
    salt: u64,
    policy: ExclusionPolicy,
}

impl Workspace {
    pub fn new(policy: ExclusionPolicy) -> Self {
        let salt = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
            ^ (std::process::id() as u64).rotate_left(32);
        Workspace {
            busy: Busy::Idle,
            session: None,
            counter: 0,
            salt,
            policy,
        }
    }

    pub fn busy(&self) -> Busy {
        self.busy
    }

    pub fn policy(&self) -> &ExclusionPolicy {
        &self.policy
    }

    pub fn current_session_id(&self) -> Option<&str> {
        self.session.as_ref().map(|s| s.id.as_str())
    }

    fn ensure_idle(&self, action: &str) -> Result<(), String> {
        match self.busy {
            Busy::Idle => Ok(()),
            Busy::Scanning => Err(format!("cannot {action} while a scan is running")),
            Busy::Cleaning => Err(format!("cannot {action} while a cleanup is running")),
        }
    }

    /// Replace the exclusion policy. Rejected during a cleanup (the running
    /// job uses a snapshot of the policy). Any existing session scanned under
    /// a different policy becomes stale.
    pub fn set_policy(&mut self, policy: ExclusionPolicy) -> Result<(), String> {
        if self.busy == Busy::Cleaning {
            return Err("cannot change exclusions while a cleanup is running".into());
        }
        self.policy = policy;
        let fp = self.policy.fingerprint();
        if let Some(s) = self.session.as_mut() {
            if s.policy_fingerprint != fp && s.stale.is_none() {
                s.stale = Some("exclusions changed since this scan; scan again before cleaning".into());
            }
        }
        Ok(())
    }

    pub fn begin_scan(&mut self) -> Result<ScanTicket, String> {
        self.ensure_idle("start a scan")?;
        self.busy = Busy::Scanning;
        // Old results are invalid from the moment a new scan starts.
        self.session = None;
        self.counter += 1;
        Ok(ScanTicket {
            session_id: format!("s{:x}-{}", self.salt & 0xffff_ffff, self.counter),
            policy: self.policy.clone(),
        })
    }

    pub fn finish_scan(&mut self, ticket: ScanTicket, output: &mut ScanOutput) {
        self.busy = Busy::Idle;
        let mut session = Session {
            id: ticket.session_id,
            sizes: output
                .summary
                .candidates
                .iter()
                .map(|c| (c.id.clone(), c.size_bytes))
                .collect(),
            records: std::mem::take(&mut output.records).into_iter().collect(),
            policy_fingerprint: ticket.policy.fingerprint(),
            stale: None,
        };
        if session.policy_fingerprint != self.policy.fingerprint() {
            session.stale = Some("exclusions changed during the scan; scan again before cleaning".into());
        }
        self.session = Some(session);
    }

    /// Release the busy flag after a failed/panicked operation.
    pub fn abort(&mut self) {
        self.busy = Busy::Idle;
    }

    pub fn begin_clean(
        &mut self,
        session_id: &str,
        ids: Vec<String>,
        acknowledged: Vec<String>,
    ) -> Result<CleanJob, String> {
        self.ensure_idle("clean")?;
        let session = self
            .session
            .as_ref()
            .filter(|s| s.id == session_id)
            .ok_or_else(|| "these results belong to an older scan; scan again".to_string())?;
        if let Some(reason) = &session.stale {
            return Err(reason.clone());
        }
        let prefix = format!("{session_id}:");
        // Only hand the cleaner records of this session; foreign ids will be
        // reported as unknown and never touched.
        let records: HashMap<_, _> = ids
            .iter()
            .filter(|id| id.starts_with(&prefix))
            .filter_map(|id| session.records.get(id).map(|r| (id.clone(), r.clone())))
            .collect();
        let job = CleanJob {
            session_id: session.id.clone(),
            ids,
            records,
            sizes: session.sizes.clone(),
            policy: self.policy.clone(),
            acknowledged: acknowledged.into_iter().collect(),
        };
        self.busy = Busy::Cleaning;
        Ok(job)
    }

    pub fn finish_clean(&mut self, job: &CleanJob, results: &[CleanItemResult]) {
        self.busy = Busy::Idle;
        // Only update the session that owns these items.
        if let Some(s) = self.session.as_mut().filter(|s| s.id == job.session_id) {
            for r in results {
                if r.status == CleanStatus::Trashed {
                    s.records.remove(&r.id);
                }
            }
        }
    }

    #[cfg(test)]
    fn record_count(&self) -> usize {
        self.session.as_ref().map_or(0, |s| s.records.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cleaner::{clean, CleanPolicy, Remover};
    use crate::safety::ProtectedPaths;
    use crate::scanner::{scan, ScanRequest};
    use std::cell::RefCell;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;
    use tempfile::tempdir;

    struct FakeRemover(RefCell<Vec<PathBuf>>);
    impl Remover for FakeRemover {
        fn remove(&self, path: &Path) -> Result<(), String> {
            self.0.borrow_mut().push(path.to_path_buf());
            fs::remove_dir_all(path).map_err(|e| e.to_string())
        }
    }

    fn write(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"data").unwrap();
    }

    fn fixture() -> tempfile::TempDir {
        let t = tempdir().unwrap();
        for app in ["a", "b"] {
            write(&t.path().join(app).join("package.json"));
            write(&t.path().join(app).join("node_modules/x.js"));
        }
        t
    }

    /// Full scan through the state machine, like the command layer does.
    fn do_scan(ws: &mut Workspace, root: &Path) -> crate::scanner::ScanSummary {
        let ticket = ws.begin_scan().unwrap();
        let req = ScanRequest {
            roots: vec![root.to_string_lossy().into_owned()],
        };
        let mut out = scan(
            &ticket.session_id,
            &req,
            &ticket.policy,
            &ProtectedPaths::empty(),
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        ws.finish_scan(ticket, &mut out);
        out.summary
    }

    fn run_job(job: &CleanJob, remover: &dyn Remover) -> Vec<CleanItemResult> {
        let protected = ProtectedPaths::empty();
        let policy = CleanPolicy {
            exclusions: &job.policy,
            protected: &protected,
            acknowledged: &job.acknowledged,
        };
        clean(&job.ids, &job.records, &job.sizes, &policy, remover)
    }

    #[test]
    fn session_ids_are_unique_per_scan() {
        let t = fixture();
        let mut ws = Workspace::new(ExclusionPolicy::default());
        let s1 = do_scan(&mut ws, t.path());
        let s2 = do_scan(&mut ws, t.path());
        assert_ne!(s1.session_id, s2.session_id);
        let ids1: HashSet<_> = s1.candidates.iter().map(|c| c.id.clone()).collect();
        assert!(s2.candidates.iter().all(|c| !ids1.contains(&c.id)));
    }

    #[test]
    fn cleanup_from_old_session_is_rejected_and_touches_nothing() {
        let t = fixture();
        let mut ws = Workspace::new(ExclusionPolicy::default());
        let old = do_scan(&mut ws, t.path());
        let new = do_scan(&mut ws, t.path());
        let old_ids: Vec<String> = old.candidates.iter().map(|c| c.id.clone()).collect();

        // Old session id → rejected outright.
        assert!(ws.begin_clean(&old.session_id, old_ids.clone(), vec![]).is_err());
        assert_eq!(ws.busy(), Busy::Idle);

        // Old ids smuggled under the new session id → treated as unknown.
        let job = ws.begin_clean(&new.session_id, old_ids, vec![]).unwrap();
        assert!(job.records.is_empty());
        let fake = FakeRemover(RefCell::new(vec![]));
        let res = run_job(&job, &fake);
        ws.finish_clean(&job, &res);
        assert!(res.iter().all(|r| r.status == CleanStatus::Skipped));
        assert!(fake.0.borrow().is_empty());
        assert_eq!(ws.record_count(), 2, "new session records must be intact");
    }

    #[test]
    fn scan_and_cleanup_are_mutually_exclusive() {
        let t = fixture();
        let mut ws = Workspace::new(ExclusionPolicy::default());
        let s = do_scan(&mut ws, t.path());
        let first = s.candidates[0].id.clone();

        let job = ws.begin_clean(&s.session_id, vec![first.clone()], vec![]).unwrap();
        // While cleaning: neither a scan nor a second cleanup may start, and
        // the exclusion policy cannot change under the running job.
        assert!(ws.begin_scan().is_err());
        assert!(ws.begin_clean(&s.session_id, vec![first.clone()], vec![]).is_err());
        assert!(ws.set_policy(ExclusionPolicy::default()).is_err());

        let fake = FakeRemover(RefCell::new(vec![]));
        let res = run_job(&job, &fake);
        ws.finish_clean(&job, &res);
        assert_eq!(res[0].status, CleanStatus::Trashed);
        // Exactly the trashed record is gone; the other is still cleanable.
        assert_eq!(ws.record_count(), 1);

        // While scanning: cleanup is rejected and records are not lost
        // because a failed begin_clean changes nothing.
        let _ticket = ws.begin_scan().unwrap();
        assert!(ws.begin_clean(&s.session_id, vec![], vec![]).is_err());
        ws.abort();
        assert_eq!(ws.busy(), Busy::Idle);
    }

    #[test]
    fn finish_clean_only_updates_owning_session() {
        let t = fixture();
        let mut ws = Workspace::new(ExclusionPolicy::default());
        let s = do_scan(&mut ws, t.path());
        let ids: Vec<String> = s.candidates.iter().map(|c| c.id.clone()).collect();
        let job = ws.begin_clean(&s.session_id, ids, vec![]).unwrap();
        // Simulate a late result for a session that has since been replaced.
        ws.abort();
        do_scan(&mut ws, t.path());
        let before = ws.record_count();
        let fake_results: Vec<CleanItemResult> = job
            .ids
            .iter()
            .map(|id| CleanItemResult {
                id: id.clone(),
                path: String::new(),
                status: CleanStatus::Trashed,
                message: String::new(),
                size_bytes: 0,
            })
            .collect();
        ws.finish_clean(&job, &fake_results);
        assert_eq!(ws.record_count(), before);
    }

    #[test]
    fn changing_exclusions_invalidates_previous_scan() {
        let t = tempdir().unwrap();
        let app = t.path().join("app");
        write(&app.join("package.json"));
        write(&app.join("vite.config.ts"));
        write(&app.join("dist/index.js"));
        write(&app.join("dist/keep/important.txt"));

        let mut ws = Workspace::new(ExclusionPolicy::default());
        let s = do_scan(&mut ws, t.path());
        let ids: Vec<String> = s.candidates.iter().map(|c| c.id.clone()).collect();
        assert_eq!(ids.len(), 1);

        let keep = app.join("dist/keep").to_string_lossy().into_owned();
        ws.set_policy(ExclusionPolicy::from_paths(&[keep]).unwrap()).unwrap();
        let err = ws.begin_clean(&s.session_id, ids.clone(), ids).unwrap_err();
        assert!(err.contains("exclusions changed"));
        assert!(app.join("dist/keep/important.txt").exists());

        // A fresh scan under the new policy blocks `dist` outright.
        let s2 = do_scan(&mut ws, t.path());
        assert!(s2.candidates[0].blocked_reason.is_some());
        let job = ws
            .begin_clean(&s2.session_id, vec![s2.candidates[0].id.clone()], vec![s2.candidates[0].id.clone()])
            .unwrap();
        let fake = FakeRemover(RefCell::new(vec![]));
        let res = run_job(&job, &fake);
        ws.finish_clean(&job, &res);
        assert_eq!(res[0].status, CleanStatus::Skipped);
        assert!(fake.0.borrow().is_empty());
        assert!(app.join("dist/keep/important.txt").exists());
    }
}
