use std::{collections::HashMap, iter::repeat};

use anyhow::bail;
use futures_util::{stream::FuturesUnordered, TryStreamExt};
use itertools::Itertools;
use scraper::{ElementRef, Html, Selector};
use scryfall::{format::Format, Card};
use serde::Serialize;

use crate::card_name::CardName;

use super::Metadata;

#[derive(Serialize, Debug, Clone, Copy)]
#[serde(rename_all = "UPPERCASE")]
enum Board {
    Md,
    Sb,
}

fn format_to_form_param(format: Format) -> anyhow::Result<&'static str> {
    Ok(match format {
        Format::Pauper => "PAU",
        Format::Legacy => "LE",
        Format::Pioneer => "PI",
        _ => bail!("unsuported format: {format}"),
    })
}

pub async fn fetch(format: Format) -> anyhow::Result<Vec<(Card, Option<Metadata>)>> {
    let url = "https://mtgtop8.com/topcards";
    let static_fields = &HashMap::from_iter([
        ("data", "1"),
        ("metagame_sel[VI]", "71"),
        ("metagame_sel[LE]", "39"),
        ("metagame_sel[MO]", "51"),
        ("metagame_sel[PI]", "193"),
        ("metagame_sel[EX]", "95"),
        ("metagame_sel[HI]", "211"),
        ("metagame_sel[ST]", "52"),
        ("metagame_sel[BL]", "85"),
        ("metagame_sel[LI]", "227"),
        ("metagame_sel[PAU]", "145"),
        ("metagame_sel[EDH]", "121"),
        ("metagame_sel[HIGH]", "180"),
        ("metagame_sel[EDHP]", "106"),
        ("metagame_sel[CHL]", "105"),
        ("metagame_sel[PEA]", "228"),
        ("metagame_sel[EDHM]", "157"),
        ("metagame_sel[ALCH]", "232"),
        ("metagame_sel[cEDH]", "240"),
        ("metagame_sel[EXP]", "259"),
        ("metagame_sel[PREM]", "261"),
        ("card_col", ""),
        ("card_type", ""),
        ("card_rarity", ""),
        ("lands", "1"),
    ]);

    #[derive(Debug, Serialize)]
    struct Form<'s> {
        current_page: String,
        maindeck: Board,
        format: &'static str,
        #[serde(flatten)]
        static_fields: &'s HashMap<&'static str, &'static str>,
    }
    let client = &reqwest::Client::new();
    let cards = ((1..=16).zip(repeat(Board::Md)))
        .chain((1..=16).zip(repeat(Board::Sb)))
        .map(|(page, board)| async move {
            println!("=> downloading page {page:02} of mtgtop8 ({board:?})");
            let text = client
                .post(url)
                .form(&Form {
                    current_page: page.to_string(),
                    format: format_to_form_param(format)?,
                    maindeck: board,
                    static_fields,
                })
                .send()
                .await?
                .text()
                .await?;

            println!("xxx downloaded page {page:02} of mtgtop8 ({board:?})");

            let doc = Html::parse_document(&text);
            let selector = Selector::parse(r#"td[class="L14"]"#).unwrap();
            let r = anyhow::Ok(
                doc.select(&selector)
                    .chunks(3)
                    .into_iter()
                    .map(|mut card| {
                        fn text_to_f(elem: &ElementRef<'_>) -> Option<f32> {
                            elem.text()
                                .next()?
                                .split_whitespace()
                                .filter(|x| !x.is_empty())
                                .map(str::parse)
                                .next()?
                                .ok()
                        }
                        let (name, percent, number_in_decks) = card.next_tuple().unwrap();
                        let name = CardName::from(name.text().collect::<String>());
                        let percent = text_to_f(&percent);
                        let num_copies = text_to_f(&number_in_decks).map(|n| n.ceil() as u8);
                        (name, Metadata::new(percent, num_copies))
                    })
                    .collect::<Vec<_>>(),
            );
            println!(
                "<===== scraped page {page:02} of mtgtop8, found {:?} cards",
                r.as_ref().map(|v| v.len())
            );
            r
        })
        .collect::<FuturesUnordered<_>>()
        .into_stream()
        .try_collect::<Vec<Vec<_>>>()
        .await?;

    cards
        .into_iter()
        .flatten()
        .map(|(card, percent)| async move { super::get_cached(&card).await.map(|c| (c, Some(percent))) })
        .collect::<FuturesUnordered<_>>()
        .into_stream()
        .try_collect::<Vec<_>>()
        .await
}
