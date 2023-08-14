mod collection_view;
pub mod panic;
mod show;
mod stats;
mod vim;

use std::{fmt::Write, sync::Arc};

use cursive::{
    backends::crossterm,
    theme::{BaseColor, Color},
    view::{Nameable, Resizable},
    views::{Dialog, EditView, TextView},
    Cursive, View,
};
use itertools::Itertools;
use scryfall::format::Format;
use std::future::Future;
use tokio::sync::mpsc::{self, error::TryRecvError, UnboundedSender};

use crate::checklist::Checklist;

use self::{
    collection_view::{collection_viewer, CardList, SortMode, CARD_LIST},
    vim::ViewExt,
};

trait CursiveExt {
    fn data(&mut self) -> &mut Data;
}

impl CursiveExt for Cursive {
    fn data(&mut self) -> &mut Data {
        self.user_data::<Data>().expect("cursive missing data")
    }
}

fn background<F>(tx_error: UnboundedSender<anyhow::Error>, task: F)
where
    F: Future<Output = anyhow::Result<()>> + Send + Sync + 'static,
{
    tokio::spawn(async move {
        if let Err(e) = task.await {
            let _ = tx_error.send(e);
        }
    });
}

fn error_dialog<E: ?Sized, F>(s: &mut Cursive, e: &E, then: F)
where
    E: std::error::Error,
    F: Fn(&mut Cursive) + 'static,
{
    s.add_layer(
        Dialog::new()
            .title("ERROR")
            .content(TextView::new(format!("{e:?}")))
            .button("Ok", move |s| {
                s.pop_layer();
                then(s)
            })
            .esq_to_quit(),
    )
}

fn information_dialog<F>(s: &mut Cursive, info: &str, then: F)
where
    F: Fn(&mut Cursive) + 'static,
{
    s.add_layer(
        Dialog::new()
            .title("Info")
            .content(TextView::new(info))
            .button("Ok", move |s| {
                s.pop_layer();
                then(s);
            })
            .esq_to_quit(),
    )
}

const MAIN_LAYOUT: &str = "main-layout";

fn save_as_dialog(missing: Vec<(usize, String, f32)>) -> impl View {
    Dialog::new().title("Save as").content(
        EditView::new()
            .on_submit(move |s, file_name| {
                let mut buf = String::new();
                for (count, name, _) in &missing {
                    writeln!(buf, "{count} {name}").unwrap();
                }
                if let Err(e) = std::fs::write(file_name, buf.as_bytes()) {
                    error_dialog(s, &e, |s| {
                        s.pop_layer();
                    });
                } else {
                    information_dialog(s, "file saved", |s| {
                        s.pop_layer();
                    });
                }
            })
            .min_width(20),
    )
}

struct Data {
    pub tx_error: UnboundedSender<anyhow::Error>,
    pub collection: Arc<Checklist>,
}

pub fn ui(collection: Checklist, format: Format) {
    let mut cursive = Cursive::new();
    let (tx_error, mut rx_error) = mpsc::unbounded_channel::<anyhow::Error>();
    cursive.with_theme(|current| {
        use cursive::theme::PaletteColor;
        current.palette[PaletteColor::Background] = Color::TerminalDefault;
        current.palette[PaletteColor::HighlightInactive] = Color::Dark(BaseColor::White);
        current.palette[PaletteColor::HighlightText] = Color::Dark(BaseColor::Black);
        current.palette[PaletteColor::Highlight] = Color::TerminalDefault;
        current.palette[PaletteColor::Primary] = Color::TerminalDefault;
        current.palette[PaletteColor::Secondary] = Color::TerminalDefault;
        current.palette[PaletteColor::Shadow] = Color::TerminalDefault;
        current.palette[PaletteColor::Tertiary] = Color::TerminalDefault;
        current.palette[PaletteColor::View] = Color::TerminalDefault;
        current.palette[PaletteColor::TitlePrimary] = Color::Dark(BaseColor::Blue);
        current.palette[PaletteColor::TitleSecondary] = Color::TerminalDefault;
    });

    let collection = Arc::new(collection);
    cursive.set_user_data(Data {
        tx_error,
        collection: collection.clone(),
    });

    let sort_mode = std::cell::Cell::new(SortMode::Collection);

    cursive.add_layer(
        Dialog::new()
            .title(format!("Lord Xander, The Collector | {format}"))
            .content(collection_viewer(collection.clone(), sort_mode.get()))
            .button("To Wishlist", |s| {
                let collection = s.data().collection.clone();
                let missing = s
                    .call_on_name::<CardList, _, _>(CARD_LIST, |view| {
                        view.iter()
                            .map(|(_, index)| {
                                let card = &collection[*index];
                                (
                                    (card.metadata.num_copies as usize)
                                        .saturating_sub(card.owned_versions().len()),
                                    card.card.name.clone(),
                                    card.metadata.percent_in_decks,
                                )
                            })
                            .filter(|(missing, _, _)| *missing > 0)
                            .sorted_unstable_by(|(_, _, percent_a), (_, _, percent_b)| {
                                percent_a.total_cmp(percent_b).reverse()
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap();
                s.add_layer(save_as_dialog(missing).esq_to_quit())
            })
            .button("Show Stattistics", |s| {
                let stats_view = stats::stats(&s.data().collection);
                s.add_layer(stats_view.esq_to_quit())
            })
            .button("Toggle Sort", move |s| {
                sort_mode.set(match sort_mode.get() {
                    SortMode::Collection => SortMode::NoCollection,
                    SortMode::NoCollection => SortMode::Collection,
                });
                s.call_on_name::<Dialog, _, _>("collection-viewer", |dialog| {
                    dialog.set_content(collection_viewer(collection.clone(), sort_mode.get()));
                });
            })
            .with_name("collection-viewer"),
    );

    cursive.set_on_post_event('q', |s| s.quit());

    let mut runner = cursive.runner(crossterm::Backend::init().unwrap());
    runner.refresh();
    while runner.is_running() {
        runner.step();
        match rx_error.try_recv() {
            Ok(error) => error_dialog(
                &mut runner,
                <anyhow::Error as AsRef<dyn std::error::Error>>::as_ref(&error),
                |_| {},
            ),
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => unreachable!(),
        }
    }
}
