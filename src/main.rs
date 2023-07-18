mod checklist;
mod collection;
mod staples;
mod ui;

use anyhow::bail;
use checklist::Checklist;
use scryfall::format::Format;
use tokio::fs::File;
use ui::panic::BACKTRACE_FILE_PATH;

const PROG_NAME: &str = env!("CARGO_PKG_NAME");

fn parse_format(arg: String) -> Option<Format> {
    use fuzzy_matcher::skim::SkimMatcherV2;
    use fuzzy_matcher::FuzzyMatcher;

    static FORMAT: [(&str, Format); 3] = [
        ("pauper", Format::Pauper),
        ("legacy", Format::Legacy),
        ("pioneer", Format::Pioneer),
    ];

    let matcher = SkimMatcherV2::default();
    FORMAT
        .iter()
        .filter_map(|(format_str, format_enum)| {
            matcher
                .fuzzy_match(format_str, &arg)
                .map(|score| (score, format_enum))
        })
        .max_by_key(|(score, _)| *score)
        .map(|(_, format)| *format)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let format = std::env::args()
        .nth(1)
        .map(parse_format)
        .unwrap_or(Some(Format::Pauper));

    let Some(format) = format else {
        bail!("invalid format");
    };

    let staples = staples::fetch(format).await?;

    let collection = collection::load().await?;

    let checklist = Checklist::new(staples, collection).await?;

    let ui_task = tokio::task::spawn_blocking(move || ui::ui(checklist, format));

    ui::panic::register_backtrace_panic_handler();

    if let Err(e) = ui_task.await {
        match e.try_into_panic() {
            Ok(panic) => {
                if let Some(panic) = panic.downcast_ref::<&str>() {
                    eprintln!("ui panicked! {panic}");
                }
                if let Ok(mut file) = File::open(BACKTRACE_FILE_PATH).await {
                    let _ = tokio::io::copy(&mut file, &mut tokio::io::stdout()).await;
                }
            }
            Err(e) => bail!(e),
        }
    }
    Ok(())
}
