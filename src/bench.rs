use crate::imece::{self, Agent, Event, Imece};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;

pub struct BenchResult {
    pub task: String,
    pub director: Agent,
    pub verdict_keep: bool,
    pub test_passed: bool,
}

pub fn run(dry_run: bool, tx: &Sender<Event>) -> std::io::Result<Vec<BenchResult>> {
    let bench_dir = imece::repo_root().join("bench");
    let tasks_dir = bench_dir.join("tasks");
    let work_dir = bench_dir.join("work");
    let pristine = bench_dir.join("pristine");

    let mut task_files: Vec<PathBuf> = std::fs::read_dir(&tasks_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    task_files.sort();

    if task_files.is_empty() {
        let _ = tx.send(Event::System("no bench tasks found".to_string()));
        return Ok(vec![]);
    }

    if pristine.exists() {
        let _ = Command::new("cp")
            .args(["-r"])
            .arg(&pristine)
            .arg(&work_dir)
            .status();
    }

    let mut results: Vec<BenchResult> = Vec::new();
    let mut engine = Imece::new(dry_run);

    for task_path in &task_files {
        let test_path = task_path.with_extension("test.sh");
        let name = task_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let _ = tx.send(Event::System(format!("bench task {name}")));
        let (director, verdict_keep) = run_one(&mut engine, task_path, tx)?;
        let test_passed = run_test(&test_path, &work_dir, tx);
        results.push(BenchResult {
            task: name,
            director,
            verdict_keep,
            test_passed,
        });
    }

    report(&results, tx);
    Ok(results)
}

fn run_one(
    engine: &mut Imece,
    task_path: &Path,
    tx: &Sender<Event>,
) -> std::io::Result<(Agent, bool)> {
    let before = engine.directed_counts();
    engine.run_task(task_path, tx)?;
    let after = engine.directed_counts();
    let director = after
        .iter()
        .find(|(a, n)| before.get(a) != Some(n))
        .map(|(a, _)| *a)
        .unwrap_or(Agent::Claude);
    let keep = engine.last_verdict_keep();
    Ok((director, keep))
}

fn run_test(test_path: &Path, _work_dir: &Path, tx: &Sender<Event>) -> bool {
    if !test_path.exists() {
        let _ = tx.send(Event::System(format!(
            "no ground truth at {}",
            test_path.display()
        )));
        return false;
    }
    let out = Command::new("sh")
        .arg(test_path)
        .current_dir(imece::repo_root())
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let _ = tx.send(Event::System("ground truth: PASS".to_string()));
            true
        }
        Ok(_) => {
            let _ = tx.send(Event::System("ground truth: FAIL".to_string()));
            false
        }
        Err(e) => {
            let _ = tx.send(Event::System(format!("ground truth: ERROR {e}")));
            false
        }
    }
}

fn report(results: &[BenchResult], tx: &Sender<Event>) {
    let _ = tx.send(Event::System(
        "=== confusion matrix (verdict vs ground truth) ===".to_string(),
    ));
    let mut tp = 0;
    let mut fp = 0;
    let mut fn_ = 0;
    let mut tn = 0;
    for r in results {
        let cell = match (r.verdict_keep, r.test_passed) {
            (true, true) => {
                tp += 1;
                "TP"
            }
            (true, false) => {
                fp += 1;
                "FP"
            }
            (false, true) => {
                fn_ += 1;
                "FN"
            }
            (false, false) => {
                tn += 1;
                "TN"
            }
        };
        let _ = tx.send(Event::System(format!(
            "{} {} director={} keep={} pass={}",
            cell, r.task, r.director.name(), r.verdict_keep, r.test_passed
        )));
    }
    let _ = tx.send(Event::System(format!(
        "summary: TP={tp} FP={fp} FN={fn_} TN={tn}"
    )));
    if tp + fp > 0 {
        let precision = tp as f64 / (tp + fp) as f64;
        let _ = tx.send(Event::System(format!(
            "precision of director review: {precision:.2}"
        )));
    }
    if tp + fn_ > 0 {
        let recall = tp as f64 / (tp + fn_) as f64;
        let _ = tx.send(Event::System(format!(
            "recall of director review: {recall:.2}"
        )));
    }
}
