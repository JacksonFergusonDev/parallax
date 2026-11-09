//! Terminal presentation stays in the CLI; core progress events remain UI-neutral.
use std::io::{self, IsTerminal};
use std::time::Duration;

use comfy_table::{Cell, ContentArrangement, Table, presets};
use console::style;
use erc_core::send::Progress;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

pub fn table(headers: &[&str]) -> Table {
    let interactive = io::stdout().is_terminal();
    let mut table = Table::new();
    table
        .load_style(if interactive {
            presets::UTF8_FULL
        } else {
            presets::NOTHING
        })
        .set_content_arrangement(if interactive {
            ContentArrangement::Dynamic
        } else {
            ContentArrangement::Disabled
        })
        .set_header(
            headers
                .iter()
                .map(|h| Cell::new(style(h).bold().to_string())),
        );
    table
}

pub fn success(label: &str) -> String {
    style(label).green().to_string()
}

pub fn warning(label: &str) -> String {
    style(label).yellow().to_string()
}

pub fn failure(label: &str) -> String {
    style(label).for_stderr().red().bold().to_string()
}

/// One operation owns one bar, clearing it even when a caller returns an error.
/// Redirected stderr keeps useful line-by-line logs; JSON suppresses progress.
pub struct Reporter {
    bar: Option<ProgressBar>,
    quiet: bool,
}

impl Reporter {
    pub fn new(json: bool) -> Self {
        let bar = (!json && io::stderr().is_terminal()).then(|| {
            let bar = ProgressBar::with_draw_target(None, ProgressDrawTarget::stderr());
            bar.set_style(
                ProgressStyle::with_template(
                    "{spinner:.cyan} [{bar:24.cyan/blue}] {pos}/{len} {wide_msg}",
                )
                .expect("valid progress template")
                .progress_chars("=>-"),
            );
            bar.enable_steady_tick(Duration::from_millis(100));
            bar
        });
        Self { bar, quiet: json }
    }

    pub fn update(&self, done: usize, total: usize, message: &str) {
        if self.quiet {
            return;
        }
        if let Some(bar) = &self.bar {
            // Metadata workers can report completions out of order.
            bar.update(|state| {
                state.set_len(total as u64);
                state.set_pos(state.pos().max(done.min(total) as u64));
            });
            bar.set_message(message.to_owned());
        } else {
            eprintln!("[{done}/{total}] {message}");
        }
    }

    pub fn send(&self, progress: Progress) {
        let (index, total, complete, message, failed) = match progress {
            Progress::Preparing { index, total, file } => {
                (index, total, false, format!("preparing {file}"), false)
            }
            Progress::Converting { index, total, file } => {
                (index, total, false, format!("converting {file}"), false)
            }
            Progress::ProcessingComic {
                index,
                total,
                file,
                page,
                pages,
            } => (
                index,
                total,
                false,
                format!("preparing {file}: page {page}/{pages}"),
                false,
            ),
            Progress::Copying {
                index,
                total,
                file,
                bytes,
                total_bytes,
            } => {
                if self.bar.is_none() && bytes > 0 {
                    return;
                }
                let percent = bytes
                    .saturating_mul(100)
                    .checked_div(total_bytes)
                    .unwrap_or(0);
                (
                    index,
                    total,
                    false,
                    format!("copying {file} ({percent}%)"),
                    false,
                )
            }
            Progress::Done {
                index,
                total,
                device_path,
                ..
            } => (index, total, true, format!("sent to {device_path}"), false),
            Progress::Failed {
                index,
                total,
                file,
                error,
            } => (
                index,
                total,
                true,
                format!("{} {file}: {error}", failure("FAILED")),
                true,
            ),
        };
        self.update(
            if complete {
                index
            } else {
                index.saturating_sub(1)
            },
            total,
            &message,
        );
        if failed && !self.quiet {
            // Preserve failures while later progress overwrites the current line.
            if let Some(bar) = &self.bar {
                bar.println(message);
            }
        }
    }
}

impl Drop for Reporter {
    fn drop(&mut self) {
        if let Some(bar) = &self.bar {
            bar.finish_and_clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_progress_counts_completed_books_including_failures() {
        let bar = ProgressBar::hidden();
        let reporter = Reporter {
            bar: Some(bar.clone()),
            quiet: false,
        };
        reporter.send(Progress::Copying {
            index: 1,
            total: 2,
            file: "one.epub".into(),
            bytes: 25,
            total_bytes: 100,
        });
        assert_eq!(bar.position(), 0);
        reporter.send(Progress::Done {
            index: 1,
            total: 2,
            file: "one.epub".into(),
            device_path: "KEpub/one.kepub.epub".into(),
        });
        assert_eq!(bar.position(), 1);
        reporter.send(Progress::Failed {
            index: 2,
            total: 2,
            file: "two.epub".into(),
            error: "conversion failed".into(),
        });
        assert_eq!(bar.position(), 2);
        assert_eq!(bar.length(), Some(2));
        drop(reporter);
        assert!(bar.is_finished());
    }

    #[test]
    fn parallel_completions_do_not_move_progress_backwards() {
        let bar = ProgressBar::hidden();
        let reporter = Reporter {
            bar: Some(bar.clone()),
            quiet: false,
        };
        reporter.update(3, 5, "third");
        reporter.update(2, 5, "second");
        assert_eq!(bar.position(), 3);
        reporter.update(99, 5, "clamped");
        assert_eq!(bar.position(), 5);
    }

    #[test]
    fn json_suppresses_progress_even_with_a_draw_target() {
        let bar = ProgressBar::hidden();
        let reporter = Reporter {
            bar: Some(bar.clone()),
            quiet: true,
        };
        reporter.update(1, 2, "hidden");
        assert_eq!(bar.position(), 0);
    }
}
