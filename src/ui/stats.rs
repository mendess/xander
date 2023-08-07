use cursive::{
    theme::Effect,
    view::Margins,
    views::{Dialog, LinearLayout, PaddedView, TextView},
    View,
};
use scryfall::card::Color;
use static_assertions::const_assert;

use crate::checklist::{Checklist, ChecklistCard};

#[derive(Default, Debug, Clone, Copy)]
struct Progress {
    owned: u16,
    total: u16,
}

#[derive(Default, Debug)]
struct Stats {
    top_20: Progress,
    top_50: Progress,
    top_150: Progress,
    top_20_by_color: [Progress; 5],
    top_10_colorless: Progress,
    top_20_multicolor: Progress,
    top_10_lands: Progress,
}

const WUBRG: [Color; 5] = [
    Color::White,
    Color::Blue,
    Color::Black,
    Color::Red,
    Color::Green,
];

fn calculate(checklist: &Checklist) -> Stats {
    let mut top_cards = Vec::with_capacity(150);

    const_assert!((Color::White as u8).trailing_zeros() == 0);
    const_assert!((Color::Blue as u8).trailing_zeros() == 1);
    const_assert!((Color::Black as u8).trailing_zeros() == 2);
    const_assert!((Color::Red as u8).trailing_zeros() == 3);
    const_assert!((Color::Green as u8).trailing_zeros() == 4);
    const COLORLESS: usize = 5;
    const MULTICOLOR: usize = 6;
    const LAND: usize = 7;
    let mut counters = [0_u16; LAND + 1];

    fn counters_full(counters: &[u16; 8]) -> bool {
        WUBRG
            .into_iter()
            .all(|c| counters[(c as u8).trailing_zeros() as usize] >= 20)
            && counters[5] >= 20
            && counters[6] >= 10
    }
    let mut iter = checklist.iter();
    while !counters_full(&counters) {
        let Some(c) = iter.next() else {
            break;
        };
        let card = &c.card;
        let index = match card.colors.as_deref() {
            _ if card.type_line.as_ref().is_some_and(|s| s.contains("Land")) => LAND,
            None | Some(&[]) => COLORLESS,
            Some(&[c]) => (c as u8).trailing_ones() as usize,
            Some(&[_, ..]) => MULTICOLOR,
        };
        counters[index] += 1;
        top_cards.push(c);
    }
    top_cards.sort_by(|a, b| a.cmp_ignoring_collected(b));

    return Stats {
        top_20: top(&top_cards, 20, |_| true),
        top_50: top(&top_cards, 50, |_| true),
        top_150: top(&top_cards, 150, |_| true),
        top_20_by_color: WUBRG.map(|color| {
            top(&top_cards, 20, |c| {
                c.card.colors.as_ref().is_some_and(|c| c == &[color])
            })
        }),
        top_10_colorless: top(&top_cards, 10, |c| {
            c.card.colors.as_ref().map(|s| s.is_empty()).unwrap_or(true)
        }),
        top_20_multicolor: top(&top_cards, 20, |c| {
            c.card.colors.as_ref().is_some_and(|s| s.len() > 1)
        }),
        top_10_lands: top(&top_cards, 10, |c| {
            c.card
                .type_line
                .as_ref()
                .is_some_and(|t| t.contains("Land"))
        }),
    };

    fn top<F: Fn(&ChecklistCard) -> bool>(
        cards: &[&ChecklistCard],
        count: usize,
        f: F,
    ) -> Progress {
        cards
            .iter()
            .filter(|x| f(x))
            .take(count)
            .fold(Progress::default(), |mut prog, c| {
                let num_copies = c.metadata.num_copies.into();
                let relevant_owned = u16::min(c.owned_versions().len() as u16, num_copies);

                prog.owned += relevant_owned;
                prog.total += num_copies;
                prog
            })
    }
}

fn stat_text(name: &str, progress: Progress) -> impl View {
    LinearLayout::vertical()
        .child(TextView::new(name).style(Effect::Bold))
        .child(
            cursive::views::ProgressBar::new()
                .max(progress.total.into())
                .with_value(cursive::utils::Counter::new(progress.owned.into()))
                .with_label(|value, (_, max)| format!("{value}/{max}"))
                .with_color(cursive::theme::Color::Light(
                    cursive::theme::BaseColor::White,
                )),
        )
}

pub fn stats(checklist: &Checklist) -> impl View {
    let stats = calculate(checklist);
    Dialog::new().content(
        LinearLayout::horizontal()
            .child(PaddedView::new(
                Margins::lrtb(1, 1, 1, 1),
                LinearLayout::vertical()
                    .child(stat_text("Top 20", stats.top_20))
                    .child(stat_text("Top 50", stats.top_50))
                    .child(stat_text("Top 150", stats.top_150)),
            ))
            .child(PaddedView::new(
                Margins::lrtb(1, 1, 1, 1),
                stats
                    .top_20_by_color
                    .iter()
                    .enumerate()
                    .map(|(i, progress)| {
                        stat_text(&format!("Top 20 {} cards", WUBRG[i]), *progress)
                    })
                    .fold(LinearLayout::vertical(), LinearLayout::child),
            ))
            .child(PaddedView::new(
                Margins::lrtb(1, 1, 1, 1),
                LinearLayout::vertical()
                    .child(stat_text("Top 10 colorless", stats.top_10_colorless))
                    .child(stat_text("Top 20 multicolor", stats.top_20_multicolor))
                    .child(stat_text("Top 10 land", stats.top_10_lands)),
            )),
    )
}
