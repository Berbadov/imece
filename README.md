# imece-tui

İmece usulü agent orchestration as a Rust TUI: a village of three CLI
agents (Claude, Codex, Mistral) harvests a task queue by mutual aid.
Each task gets a rotating DIRECTOR who plans and reviews; the other two
are HELPERS who contribute competing diffs. No subagents, no router-god.

## Run

    cargo run -- --dry-run        # fake agents, watch the village chat
    cargo run                     # real: needs claude/codex/mistral CLIs
    cargo run -- --benchmark      # ground-truth eval, headless
    cargo run -- --no-tui         # headless plain output

`q` / Ctrl-C quits.

## UI

Chat-style transcript of the village at work, with each model's own
brand mark and color:

    Claude  ✳ sunburst  coral   # D97757
    Codex   ⬡ hex knot   teal    # 10A37F
    Mistral M  block-M   amber   # FAC83C

Sidebar shows each villager's live state with animated brand sprites:
spinning rays (thinking), cycling hex nodes (working), sweeping block
bars (talking), slow breathing when idle.

## Layout

    src/imece.rs      engine: rotation, plan -> peer diffs -> review, git apply
    src/classifier.rs field classification (Jev-shaped, keyword fallback)
    src/bench.rs      --benchmark: ground-truth confusion matrix
    src/ui.rs         chat UI + animated brand sprites
    tasks/            task queue (one .md per task)
    runs/             per-task work logs
