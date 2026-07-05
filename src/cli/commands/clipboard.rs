use crate::clipboard::{
    copy_to_system_clipboard, wipe_system_clipboard, ClipItem, ClipboardStore, CopyOutcome,
};
use anyhow::Result;
use clap::Subcommand;
use std::io::Read;

#[derive(Subcommand, Debug)]
pub enum ClipboardCommands {
    /// Push a snippet onto the queue (reads stdin when no content given)
    #[command(visible_alias = "p")]
    Push {
        /// Snippet content; omit to read from stdin (e.g. `cat f.txt | acta cb p`)
        content: Option<String>,

        /// Short human-readable description shown in `list`/`next`
        #[arg(short, long)]
        desc: Option<String>,

        /// Mark as sensitive: content is masked in `list` output
        #[arg(short, long)]
        sensitive: bool,
    },

    /// Pop the next snippet onto the system clipboard
    #[command(visible_alias = "n")]
    Next {
        /// Print the snippet instead of copying it (for headless shells)
        #[arg(long)]
        stdout: bool,
    },

    /// Show the head of the queue without popping it
    Peek,

    /// List queued snippets
    #[command(visible_alias = "ls")]
    List,

    /// Drop the head of the queue without copying it
    Skip,

    /// Empty the queue
    #[command(visible_alias = "cl")]
    Clear {
        /// Also overwrite the system clipboard (wipe sensitive residue)
        #[arg(short, long)]
        sensitive: bool,
    },
}

pub async fn execute(command: ClipboardCommands) -> Result<()> {
    let store = ClipboardStore::new()?;
    match command {
        ClipboardCommands::Push {
            content,
            desc,
            sensitive,
        } => {
            let content = match content {
                Some(content) => content,
                None => {
                    let mut buf = String::new();
                    std::io::stdin().read_to_string(&mut buf)?;
                    buf
                }
            };
            if content.is_empty() {
                anyhow::bail!("Refusing to push an empty snippet");
            }
            let item = store.push(content, desc, sensitive)?;
            let remaining = store.list()?.len();
            println!(
                "📋 Queued #{}{} ({} in queue) — pop with `acta cb next`",
                item.id,
                item.description
                    .as_deref()
                    .map(|d| format!(": {d}"))
                    .unwrap_or_default(),
                remaining
            );
        }
        ClipboardCommands::Next { stdout } => {
            let (popped, upcoming) = store.next()?;
            let Some(item) = popped else {
                println!("Clipboard queue is empty");
                return Ok(());
            };
            if stdout {
                print!("{}", item.content);
                return Ok(());
            }
            match copy_to_system_clipboard(&item.content) {
                CopyOutcome::Native(tool) => {
                    println!(
                        "📋 #{} copied to clipboard via {tool}{}",
                        item.id,
                        describe(&item)
                    );
                }
                CopyOutcome::Osc52 => {
                    println!(
                        "📋 #{} sent to your terminal's clipboard (OSC 52){}",
                        item.id,
                        describe(&item)
                    );
                }
                CopyOutcome::Unavailable => {
                    eprintln!("⚠️  No clipboard tool found (pbcopy/wl-copy/xclip/xsel) and no tty for OSC 52.");
                    eprintln!("    Printing instead:\n");
                    println!("{}", item.content);
                }
            }
            match upcoming {
                Some(next) => println!("   Next up: {}", preview(&next)),
                None => println!("   Queue is now empty"),
            }
        }
        ClipboardCommands::Peek => match store.peek()? {
            Some(item) => println!("{}", preview(&item)),
            None => println!("Clipboard queue is empty"),
        },
        ClipboardCommands::List => {
            let items = store.list()?;
            if items.is_empty() {
                println!("Clipboard queue is empty");
                return Ok(());
            }
            println!("{} item(s) queued — oldest pops first:", items.len());
            for (position, item) in items.iter().enumerate() {
                println!("  {}. {}", position + 1, preview(item));
            }
        }
        ClipboardCommands::Skip => match store.skip()? {
            Some(item) => println!("⏭️  Dropped {}", preview(&item)),
            None => println!("Clipboard queue is empty"),
        },
        ClipboardCommands::Clear { sensitive } => {
            let count = store.clear()?;
            print!("🧹 Cleared {count} item(s)");
            if sensitive {
                match wipe_system_clipboard() {
                    CopyOutcome::Unavailable => print!(" (could not wipe system clipboard)"),
                    _ => print!(" and wiped the system clipboard"),
                }
            }
            println!();
        }
    }
    Ok(())
}

fn describe(item: &ClipItem) -> String {
    item.description
        .as_deref()
        .map(|d| format!(" — {d}"))
        .unwrap_or_default()
}

fn preview(item: &ClipItem) -> String {
    // Never print sensitive content — only its description, if any.
    let label = if item.sensitive {
        item.description
            .clone()
            .unwrap_or_else(|| "(sensitive)".into())
    } else {
        item.description
            .clone()
            .unwrap_or_else(|| single_line_preview(&item.content, 60))
    };
    let mut parts = format!("#{} {label}", item.id);
    if item.sensitive {
        parts.push_str(" 🔒");
    } else if item.description.is_some() {
        parts.push_str(&format!(" · {}", single_line_preview(&item.content, 40)));
    }
    if let Some(source) = &item.source {
        parts.push_str(&format!(" (from {source})"));
    }
    parts
}

fn single_line_preview(content: &str, max: usize) -> String {
    let flat: String = content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ⏎ ");
    let mut preview: String = flat.chars().take(max).collect();
    if flat.chars().count() > max {
        preview.push('…');
    }
    preview
}
