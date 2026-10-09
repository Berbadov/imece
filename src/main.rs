mod bench;
mod classifier;
mod imece;
mod ui;

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use imece::Event;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{Event as CEvent, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::Terminal;
use ui::Chat;

fn main() -> io::Result<()> {
    let dry_run = std::env::args().any(|a| a == "--dry-run");
    let benchmark = std::env::args().any(|a| a == "--benchmark");
    let no_tui = benchmark
        || std::env::args().any(|a| a == "--no-tui")
        || !atty::is(atty::Stream::Stdout);
    let task = std::env::args()
        .position(|a| a == "--task")
        .and_then(|i| std::env::args().nth(i + 1));

    let (tx, rx) = mpsc::channel::<Event>();
    let worker_done = Arc::new(AtomicBool::new(false));
    let done_flag = worker_done.clone();

    if no_tui {
        let worker_tx = tx.clone();
        thread::spawn(move || {
            run_village(dry_run, benchmark, task, &worker_tx);
            done_flag.store(true, Ordering::SeqCst);
        });
        drop(tx);
        loop {
            match rx.recv() {
                Ok(ev) => match ev {
                    Event::System(t) => println!("[imece] {t}"),
                    Event::Said(a, t) => println!("[{}]: {t}", a.name()),
                    other => println!("{other:?}"),
                },
                Err(_) => break,
            }
        }
        return Ok(());
    }

    thread::spawn(move || {
        run_village(dry_run, benchmark, task, &tx);
        done_flag.store(true, Ordering::SeqCst);
    });

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut chat = Chat::new();
    let mut quit = false;
    let mut idle = 0u32;

    while !quit {
        let received = drain(&rx, &mut chat);
        let ticked = chat.tick();

        terminal.draw(|f| {
            chat.render(f.area(), f);
        })?;

        if worker_done.load(Ordering::SeqCst) && !received && !ticked {
            idle += 1;
        } else {
            idle = 0;
        }
        if idle > 20 {
            break;
        }

        while ratatui::crossterm::event::poll(Duration::from_millis(120))? {
            if let CEvent::Key(key) = ratatui::crossterm::event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') => quit = true,
                    KeyCode::Char('c')
                        if key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        quit = true;
                    }
                    KeyCode::Up => chat.scroll_up(),
                    KeyCode::Down => chat.scroll_down(),
                    KeyCode::Char('g') => chat.scroll_top(),
                    KeyCode::Char('G') => chat.scroll_bottom(),
                    KeyCode::Char(' ') => chat.toggle_pause(),
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    println!("imece finished - logs in ./runs");
    Ok(())
}

fn run_village(
    dry_run: bool,
    benchmark: bool,
    task: Option<String>,
    tx: &mpsc::Sender<Event>,
) {
    if benchmark {
        let _ = bench::run(dry_run, tx);
        return;
    }
    let mut engine = imece::Imece::new(dry_run);
    let tasks = imece::load_tasks(task.as_deref());
    if tasks.is_empty() {
        let _ = tx.send(Event::System("no tasks found in ./tasks (or --task path)".to_string()));
        return;
    }
    if !dry_run && !imece::git_is_clean(tx) {
        return;
    }
    let _ = tx.send(Event::System(format!("queue {} tasks", tasks.len())));
    for t in &tasks {
        if let Err(e) = engine.run_task(t, tx) {
            let _ = tx.send(Event::System(format!("task failed: {e}")));
        }
    }
    let _ = tx.send(Event::System("village done.".to_string()));
}

fn drain(rx: &mpsc::Receiver<Event>, chat: &mut Chat) -> bool {
    let mut received = false;
    while let Ok(ev) = rx.try_recv() {
        chat.push(ev);
        received = true;
    }
    received
}
