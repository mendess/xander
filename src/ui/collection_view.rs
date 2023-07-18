use std::sync::Arc;

use cursive::{
    event::{Callback, Event, EventResult, Key},
    theme::{BaseColor, Color, ColorStyle, ColorType},
    utils::{span::SpannedString, Counter},
    view::{Nameable, Resizable, Scrollable},
    views::{Dialog, EditView, LinearLayout, OnEventView, ProgressBar, ScrollView, SelectView},
    Cursive, View,
};
use scryfall::set::SetCode;

use crate::checklist::{Checklist, ChecklistCard};

use super::{background, show, vim::ViewExt, CursiveExt, MAIN_LAYOUT};

pub const CARD_LIST: &str = "card-list";
pub const CARD_LIST_SCROLL_VIEW: &str = "card-list-scroll-view";
pub type CardList = SelectView<usize>;
pub type CardListScrollView = ScrollView<LinearLayout>;
const PROGRESS_VIEWER: &str = "progress-viewer";
const VERSION_VIEWER: &str = "versions-viewer";

// pub struct CollectionView {
//     checklist: Checklist,
//     view: LinearLayout,
// }

fn mtg_color_to_bar_color(color: Option<&[scryfall::card::Color]>) -> cursive::theme::Color {
    use scryfall::card::Color::*;
    match color {
        Some([White]) => Color::Light(BaseColor::White),
        Some([Blue]) => Color::Light(BaseColor::Blue),
        Some([Black]) => Color::Light(BaseColor::Black),
        Some([Red]) => Color::Light(BaseColor::Red),
        Some([Green]) => Color::Light(BaseColor::Green),
        Some(multi) if multi.len() > 1 => Color::Light(BaseColor::Yellow),
        _ => Color::RgbLowRes(3, 2, 1),
    }
}

fn get_selected_card_name(s: &mut Cursive) -> String {
    let collection = s.data().collection.clone();
    s.call_on_name::<CardList, _, _>(CARD_LIST, |card_list| {
        let index = card_list.selected_id().unwrap();
        let col_index = card_list.get_item(index).unwrap();
        collection[*col_index.1].card.name.clone()
    })
    .expect(CARD_LIST)
}

fn add_collected_version(s: &mut Cursive, version: SetCode) {
    let collection = s.data().collection.clone();
    let (index, len) = s
        .call_on_name::<CardList, _, _>(CARD_LIST, |card_list| {
            let index = card_list.selected_id().unwrap();
            let len = {
                let card = card_list
                    .get_item_mut(index)
                    .map(|(_, index)| &collection[*index])
                    .unwrap();
                card.add_version(version)
            };
            (index, len)
        })
        .expect(CARD_LIST);
    s.call_on_name::<SelectView<SetCode>, _, _>(VERSION_VIEWER, |set_codes| {
        set_codes.add_item(version.to_string(), version);
    })
    .expect(VERSION_VIEWER);
    s.call_on_name::<LinearLayout, _, _>(PROGRESS_VIEWER, |collection_viewer| {
        let progress = collection_viewer
            .get_child_mut(index)
            .unwrap()
            .downcast_mut::<ProgressBar>()
            .unwrap();
        progress.set_value(len);
    })
    .expect(PROGRESS_VIEWER);
}

fn del_collected_version(s: &mut Cursive, version: SetCode) {
    let collection = s.data().collection.clone();
    let (index, len) = s
        .call_on_name::<CardList, _, _>(CARD_LIST, |card_list| {
            let index = card_list.selected_id().unwrap();
            let len = {
                let (_, index) = card_list.get_item_mut(index).unwrap();
                collection[*index].remove_version(version)
            };
            (index, len)
        })
        .expect(CARD_LIST);
    s.call_on_name::<SelectView<SetCode>, _, _>(VERSION_VIEWER, |set_codes| {
        let selected = set_codes.selected_id().unwrap();
        set_codes.remove_item(selected);
    })
    .expect(VERSION_VIEWER);
    s.call_on_name::<LinearLayout, _, _>(PROGRESS_VIEWER, |collection_viewer| {
        let progress = collection_viewer
            .get_child_mut(index)
            .unwrap()
            .downcast_mut::<ProgressBar>()
            .unwrap();
        progress.set_value(len);
    })
    .expect(PROGRESS_VIEWER);
}

fn edit_collected_card_dialog(card: &ChecklistCard) -> impl View {
    let mut versions_view = SelectView::new();

    for version in card.versions().iter() {
        versions_view.add_item(version.to_string(), *version);
    }

    versions_view.set_on_submit(|s, item| {
        let selected = get_selected_card_name(s);
        background(
            s.data().tx_error.clone(),
            crate::collection::del_from_collection(selected, *item),
        );
        del_collected_version(s, *item)
    });

    let printings = card.printings.clone();

    Dialog::new()
        .title(&card.card.name)
        .content(versions_view.with_name(VERSION_VIEWER))
        .button("Done", |s| {
            s.pop_layer();
        })
        .button("Add", move |s| {
            let mut set_picker = SelectView::new();
            for &set in &printings {
                set_picker.add_item(set.to_string(), set);
            }
            set_picker.set_on_submit(|s, &set| {
                let selected = get_selected_card_name(s);
                background(
                    s.data().tx_error.clone(),
                    crate::collection::add_to_collection(selected, set),
                );
                add_collected_version(s, set);
                s.pop_layer();
            });
            s.add_layer(
                Dialog::new()
                    .title("Add Card Version")
                    .content(set_picker.scrollable().with_vim_keys().esq_to_quit()),
            );
        })
        .esq_to_quit()
        .with_vim_keys()
}

pub fn collection_viewer(collection: Arc<Checklist>) -> impl View {
    let mut names = SelectView::new();
    let mut progress = LinearLayout::vertical();
    let max_text_width = collection
        .iter()
        .map(|c| c.card.name.len())
        .max()
        .unwrap_or_default();
    for (index, card) in collection.iter().enumerate() {
        let metadata = card.metadata;
        progress.add_child(
            ProgressBar::new()
                .min(0)
                .max(4)
                .with_value(Counter::new(card.versions().len()))
                .with_label(move |value, _| {
                    format!(
                        "{value}/{} ({}%)",
                        metadata.num_copies, metadata.percent_in_decks
                    )
                })
                .with_color(mtg_color_to_bar_color(card.card.colors.as_deref())),
        );
        let styled = SpannedString::styled(
            format!("{:max_text_width$}", card.card.name),
            ColorStyle {
                front: ColorType::InheritParent,
                back: ColorType::InheritParent,
            },
        );
        names.add_item(styled, index);
    }

    let names = OnEventView::new(
        names
            .on_submit({
                let collection = collection.clone();
                move |s, index| {
                    let card = &collection[*index];
                    s.add_layer(edit_collected_card_dialog(card))
                }
            })
            .with_name(CARD_LIST),
    )
    .on_pre_event('G', |s| {
        do_with_cardlist(
            s,
            |view| view.set_selection(view.len()),
            |view| view.scroll_to_bottom(),
        )
    })
    .on_pre_event('g', |s| {
        do_with_cardlist(s, |view| view.set_selection(0), |view| view.scroll_to_top())
    })
    // .on_pre_event_inner('c', |view, _| {
    //     1;
    //     Some(EventResult::Ignored)
    // })
    .on_pre_event_inner(Event::Char('s'), {
        move |view, _| {
            let view = view.get_mut();
            if let Some(show_task) = view
                .selected_id()
                .and_then(|idx| view.get_item(idx))
                .and_then(|(_, index)| show::show(&collection[*index].card))
            {
                Some(EventResult::Consumed(Some(Callback::from_fn_once(|s| {
                    background(s.data().tx_error.clone(), show_task)
                }))))
            } else {
                Some(EventResult::Consumed(None))
            }
        }
    });

    LinearLayout::vertical()
        .child(
            OnEventView::new(
                LinearLayout::horizontal()
                    .child(names)
                    .child(progress.with_name(PROGRESS_VIEWER).min_width(20))
                    .scrollable()
                    .with_name(CARD_LIST_SCROLL_VIEW)
                    .with_vim_keys(),
            )
            .on_event(Event::Char('/'), |s| {
                let cb = s
                    .call_on_name::<LinearLayout, _, _>(MAIN_LAYOUT, |view| {
                        view.add_child(search_box());
                        let r = view.set_focus_index(1).expect("can't focus");
                        match r {
                            EventResult::Ignored => None,
                            EventResult::Consumed(c) => c,
                        }
                    })
                    .expect("Failed to find MAIN_LAYOUT");
                if let Some(cb) = cb {
                    (cb)(s)
                }
            }),
        )
        .with_name(MAIN_LAYOUT)
}

fn do_with_cardlist<Cards, C, Scroll, S>(s: &mut Cursive, card_cb: Cards, scroll: Scroll)
where
    Cards: FnOnce(&mut CardList) -> C,
    Scroll: FnOnce(&mut CardListScrollView) -> S,
{
    s.call_on_name::<CardList, _, _>(CARD_LIST, card_cb)
        .expect(CARD_LIST);
    s.call_on_name::<CardListScrollView, _, _>(CARD_LIST_SCROLL_VIEW, scroll)
        .expect(CARD_LIST_SCROLL_VIEW);
}

fn search_box() -> impl View {
    fn quit(s: &mut Cursive) {
        s.call_on_name::<LinearLayout, _, _>(MAIN_LAYOUT, |view| view.remove_child(1))
            .expect("Failed to find MAIN_LAYOUT");
    }

    OnEventView::new(
        Dialog::new()
            .content(
                EditView::new()
                    .on_edit(|s, text, _cursor| {
                        use fuzzy_matcher::skim::SkimMatcherV2;
                        use fuzzy_matcher::FuzzyMatcher;

                        let matcher = SkimMatcherV2::default();
                        do_with_cardlist(
                            s,
                            |view| {
                                let index = view
                                    .iter()
                                    .enumerate()
                                    .filter_map(|(index, (label, _))| {
                                        matcher.fuzzy_match(label, text).map(|score| (score, index))
                                    })
                                    .max_by_key(|(score, _)| *score)
                                    .map(|(_, index)| index);

                                if let Some(index) = index {
                                    view.set_selection(index);
                                };
                            },
                            |view| view.scroll_to_important_area(),
                        );
                    })
                    .on_submit(|s, _| quit(s)),
            )
            .min_height(3)
            .max_height(3),
    )
    .on_pre_event(Key::Esc, quit)
}
