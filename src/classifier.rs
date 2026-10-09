use crate::imece::Agent;
use std::collections::HashMap;

pub struct Classifier {
    pub use_jev: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    Rust,
    Frontend,
    Docs,
    General,
}

impl Field {
    pub fn label(&self) -> &'static str {
        match self {
            Field::Rust => "rust",
            Field::Frontend => "frontend",
            Field::Docs => "docs",
            Field::General => "general",
        }
    }
}

impl Classifier {
    pub fn new() -> Self {
        Self {
            use_jev: std::env::var("JEV_API_KEY").is_ok(),
        }
    }

    pub fn classify(&self, task_text: &str) -> Field {
        if self.use_jev {
            if let Some(field) = self.jev_classify(task_text) {
                return field;
            }
        }
        keyword_classify(task_text)
    }

    fn jev_classify(&self, _task_text: &str) -> Option<Field> {
        None
    }
}

fn keyword_classify(task_text: &str) -> Field {
    let t = task_text.to_lowercase();
    let mut scores: HashMap<Field, u32> = HashMap::new();
    let bump = |scores: &mut HashMap<Field, u32>, f: Field, words: &[&str]| {
        for w in words {
            if t.contains(w) {
                *scores.entry(f).or_insert(0) += 1;
            }
        }
    };
    bump(
        &mut scores,
        Field::Rust,
        &["rust", "cargo", "crate", "borrow", "lifetime", "tokio"],
    );
    bump(
        &mut scores,
        Field::Frontend,
        &["css", "html", "react", "ui", "layout", "component"],
    );
    bump(&mut scores, Field::Docs, &["readme", "doc", "comment", "changelog"]);
    scores
        .into_iter()
        .max_by_key(|(f, n)| (*n, f.label().len()))
        .filter(|(_, n)| *n > 0)
        .map(|(f, _)| f)
        .unwrap_or(Field::General)
}

pub fn competence_bias(field: &Field) -> Vec<Agent> {
    match field {
        Field::Rust => vec![Agent::Codex, Agent::Claude, Agent::Mistral],
        Field::Frontend => vec![Agent::Claude, Agent::Mistral, Agent::Codex],
        Field::Docs => vec![Agent::Mistral, Agent::Claude, Agent::Codex],
        Field::General => vec![],
    }
}
