use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::classifier;

pub const PLAN_PROMPT: &str = r#"You are the DIRECTOR of an imece work crew on this task.
Read the task below and produce a short plan: which files to touch and what
change to make. 3 lines max. The other crew members will implement it as a
unified diff; be concrete so they don't drift.

TASK:
{task}
"#;

pub const WORK_PROMPT: &str = r#"You are a HELPER in an imece work crew. The director has
planned this task. Produce a unified diff (in a ```diff fenced block)
implementing the plan against the current repo tree. Output ONLY the diff
block plus at most one line of context. Do not apply anything.

TASK:
{task}

DIRECTOR'S PLAN ({director}):
{plan}
"#;

pub const REVIEW_PROMPT: &str = r#"You are the DIRECTOR reviewing a helper's diff for this
task. Answer with one short sentence, then a final line exactly:
VERDICT: keep   (applies cleanly, matches the plan, fixes the task)
VERDICT: reject (does not apply / wrong approach / incomplete)

TASK:
{task}

PLAN:
{plan}

DIFF from helper {helper}:
```diff
{diff}
```
"#;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Agent {
    Claude,
    Codex,
    Mistral,
}

impl Agent {
    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::Mistral => "mistral",
        }
    }

    pub fn cli(self, prompt: &str) -> Command {
        let mut cmd = match self {
            Agent::Claude => {
                let mut c = Command::new("claude");
                c.arg("-p").arg(prompt);
                c
            }
            Agent::Codex => {
                let mut c = Command::new("codex");
                c.arg("exec").arg(prompt);
                c
            }
            Agent::Mistral => {
                let mut c = Command::new("mistral");
                c.arg("chat").arg(prompt);
                c
            }
        };
        cmd.current_dir(repo_root());
        cmd
    }
}

pub const VILLAGE: [Agent; 3] = [Agent::Claude, Agent::Codex, Agent::Mistral];

/// Events streamed to the UI. Every villager action the chat should show.
#[derive(Clone, Debug)]
pub enum Event {
    /// a villager started thinking/planning
    Thinking(Agent),
    /// a villager is working (producing a diff)
    Working(Agent),
    /// a villager said something (chat bubble)
    Said(Agent, String),
    /// director-of-the-task announcement
    Director(Agent),
    /// helper contributed a diff
    Contribution(Agent, usize),
    /// director verdict on a helper's diff
    Review(Agent, Agent, bool, String),
    /// applied a helper's diff
    Applied(Agent),
    /// task-level or system line (divider style in chat)
    System(String),
    /// no diff kept / task needs human eyes
    NoKeep,
}

pub struct Imece {
    pub dry_run: bool,
    directed: HashMap<Agent, u32>,
    fake_counts: HashMap<Agent, u32>,
    last_keep: bool,
}

impl Imece {
    pub fn new(dry_run: bool) -> Self {
        Self {
            dry_run,
            directed: HashMap::new(),
            fake_counts: HashMap::new(),
            last_keep: false,
        }
    }

    pub fn directed_counts(&self) -> HashMap<Agent, u32> {
        self.directed.clone()
    }

    pub fn last_verdict_keep(&self) -> bool {
        self.last_keep
    }

    fn invoke(&mut self, agent: Agent, prompt: &str, tx: &Sender<Event>) -> String {
        if self.dry_run {
            let n = self.fake_counts.entry(agent).or_insert(0);
            *n += 1;
            let n = *n;
            return if prompt.contains("produce a short plan") {
                format!(
                    "plan from {name}: edit src/greet.py, tighten message",
                    name = agent.name()
                )
            } else if prompt.contains("unified diff") {
                format!(
                    "here is my contribution\n```diff\n--- a/src/greet.py\n+++ b/src/greet.py\n@@ -1 +1 @@\n-print('hello')\n+print('hello from {name} #{n}')\n```",
                    name = agent.name(),
                    n = n
                )
            } else if prompt.contains("reviewing a helper's diff") {
                "looks reasonable, applies cleanly.\nVERDICT: keep".to_string()
            } else {
                format!("ok ({})", agent.name())
            };
        }
        let _ = tx.send(Event::Thinking(agent));
        let output = agent
            .cli(prompt)
            .stdin(Stdio::null())
            .output()
            .map(|o| {
                let mut s = String::from_utf8_lossy(&o.stdout).to_string();
                s.push_str(&String::from_utf8_lossy(&o.stderr));
                s
            })
            .unwrap_or_else(|e| format!("[imece] failed to invoke {}: {e}", agent.name()));
        output
    }

    fn next_director(&self, field: Option<&classifier::Field>) -> Agent {
        let counts: Vec<(Agent, u32)> = VILLAGE
            .iter()
            .map(|a| (*a, self.directed.get(a).copied().unwrap_or(0)))
            .collect();
        let min = counts.iter().map(|(_, n)| *n).min().unwrap_or(0);
        let mut overdue: Vec<Agent> = counts
            .iter()
            .filter(|(_, n)| *n == min)
            .map(|(a, _)| *a)
            .collect();
        if let Some(field) = field {
            let bias = classifier::competence_bias(field);
            if !bias.is_empty() {
                overdue.sort_by_key(|a| bias.iter().position(|b| b == a).unwrap_or(bias.len()));
            }
        }
        *overdue.first().unwrap()
    }

    pub fn run_task(&mut self, task_path: &Path, tx: &Sender<Event>) -> std::io::Result<()> {
        self.last_keep = false;
        let task_text = std::fs::read_to_string(task_path)?;
        let task_text = task_text.trim().to_string();
        let classifier = classifier::Classifier::new();
        let field = classifier.classify(&task_text);

        let director = self.next_director(Some(&field));
        *self.directed.entry(director).or_insert(0) += 1;
        let helpers: Vec<Agent> = VILLAGE.iter().copied().filter(|a| *a != director).collect();

        let _ = tx.send(Event::System(format!(
            "harvesting {} - field: {}",
            task_path.file_stem().unwrap_or_default().to_string_lossy(),
            field.label()
        )));
        let _ = tx.send(Event::Director(director));

        let plan = self.invoke(director, &PLAN_PROMPT.replace("{task}", &task_text), tx);
        let _ = tx.send(Event::Said(director, plan.trim().to_string()));

        let mut contributions: Vec<(Agent, String)> = Vec::new();
        for helper in helpers {
            let _ = tx.send(Event::Working(helper));
            let prompt = WORK_PROMPT
                .replace("{task}", &task_text)
                .replace("{director}", director.name())
                .replace("{plan}", &plan);
            let out = self.invoke(helper, &prompt, tx);
            match extract_diff(&out) {
                Some(diff) => {
                    let _ = tx.send(Event::Contribution(helper, diff.len()));
                    contributions.push((helper, diff));
                }
                None => {
                    let _ = tx.send(Event::Said(helper, "(no diff produced)".to_string()));
                }
            }
        }

        let mut applied: Option<(Agent, String)> = None;
        for (helper, diff) in &contributions {
            let check_err = if self.dry_run { None } else { git_apply_check(diff) };
            if let Some(err) = check_err {
                let _ = tx.send(Event::Review(
                    director,
                    *helper,
                    false,
                    format!("does not apply: {err}"),
                ));
                continue;
            }
            let prompt = REVIEW_PROMPT
                .replace("{task}", &task_text)
                .replace("{plan}", &plan)
                .replace("{helper}", helper.name())
                .replace("{diff}", diff);
            let review = self.invoke(director, &prompt, tx);
            let keep = review.to_lowercase().contains("verdict: keep");
            let first = review.lines().next().unwrap_or("").trim().to_string();
            let _ = tx.send(Event::Review(director, *helper, keep, first));
            if keep && applied.is_none() {
                applied = Some((*helper, diff.clone()));
            }
        }

        match applied {
            Some((helper, diff)) => {
                if !self.dry_run {
                    git_apply(&diff)?;
                }
                self.last_keep = true;
                let _ = tx.send(Event::Applied(helper));
            }
            None => {
                let _ = tx.send(Event::NoKeep);
            }
        }

        write_log(task_path, director)
    }
}

pub fn repo_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn extract_diff(text: &str) -> Option<String> {
    let start = text.find("```diff\n")? + "```diff\n".len();
    let rest = &text[start..];
    let end = rest.find("```")?;
    let diff = rest[..end].trim();
    if diff.is_empty() { None } else { Some(diff.to_string()) }
}

fn git_apply_check(diff: &str) -> Option<String> {
    let mut child = Command::new("git")
        .args(["apply", "--check", "-"])
        .current_dir(repo_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(diff.as_bytes());
    }
    let out = child.wait_with_output().ok()?;
    if out.status.success() {
        None
    } else {
        Some(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn git_apply(diff: &str) -> std::io::Result<()> {
    let mut child = Command::new("git")
        .args(["apply", "-"])
        .current_dir(repo_root())
        .stdin(Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(diff.as_bytes())?;
    }
    let status = child.wait()?;
    if !status.success() {
        return Err(std::io::Error::other("git apply failed"));
    }
    Ok(())
}

fn write_log(task_path: &Path, director: Agent) -> std::io::Result<()> {
    let runs = repo_root().join("runs");
    std::fs::create_dir_all(&runs)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = runs.join(format!(
        "{}-{}.log",
        task_path.file_stem().unwrap_or_default().to_string_lossy(),
        director.name()
    ));
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(f, "[started] unix={stamp} director={}", director.name())?;
    Ok(())
}

pub fn git_is_clean(tx: &Sender<Event>) -> bool {
    let out = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_root())
        .output();
    match out {
        Ok(o) if o.status.success() => {
            if o.stdout.iter().all(|b| b.is_ascii_whitespace()) {
                true
            } else {
                let _ = tx.send(Event::System(
                    "git tree is dirty - commit or stash first".to_string(),
                ));
                false
            }
        }
        _ => true,
    }
}

pub fn load_tasks(limit: Option<&str>) -> Vec<PathBuf> {
    if let Some(p) = limit {
        let path = PathBuf::from(p);
        return if path.exists() { vec![path] } else { vec![] };
    }
    let dir = repo_root().join("tasks");
    let mut tasks: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "md"))
            .collect(),
        Err(_) => vec![],
    };
    tasks.sort();
    tasks
}

pub const _SPIN_MS: u64 = 80;
pub const _TICK: Duration = Duration::from_millis(80);
