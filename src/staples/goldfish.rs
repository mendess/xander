use anyhow::{bail, Context};
use futures_util::{
    stream::{self, FuturesUnordered},
    StreamExt, TryStreamExt,
};
use reqwest::Url;
use scraper::{Html, Selector};
use scryfall::{format::Format, Card};

use super::Metadata;

fn urls_from_format(format: Format) -> anyhow::Result<[Url; 3]> {
    let format = match format {
        Format::Pauper => "pauper",
        Format::Pioneer => "pioneer",
        Format::Legacy => "legacy",
        Format::Standard => "standard",
        _ => bail!("{format} not supported"),
    };

    Ok(["creatures", "spells", "lands"].map(|ty| {
        Url::parse(&format!(
            "https://www.mtggoldfish.com/format-staples/{format}/full/{ty}"
        ))
        .unwrap()
    }))
}

pub async fn scrape(url: Url) -> anyhow::Result<Vec<anyhow::Result<(Card, Metadata)>>> {
    let url_str = url.to_string();
    let html = reqwest::get(url).await?.text().await?;
    println!("{url_str} downloaded");
    let doc = Html::parse_document(&html);
    let table = Selector::parse("table").unwrap();
    if let Some(table) = doc.select(&table).next() {
        let tr = Selector::parse("tr").unwrap();
        Ok(table
            .select(&tr)
            .filter(|e| {
                let parent = e
                    .parent()
                    .and_then(|parent| parent.value().as_element())
                    .map(|parent| parent.name());
                parent != Some("thead")
            })
            .map(|e| async move {
                let mut values = e.text().map(str::trim).filter(|s| !s.is_empty()).skip(1);
                let name = values.next().unwrap().into();
                let percent_in_decks = values
                    .next()
                    .and_then(|s| s.trim_end_matches('%').parse().ok());
                let num_copies = values
                    .next()
                    .and_then(|s| s.parse::<f32>().ok())
                    .map(|c| c.ceil() as u8);

                let card = super::get_cached(name)
                    .await
                    .context("fetching from goldfish");
                card.map(|card| (card, Metadata::new(percent_in_decks, num_copies)))
            })
            .collect::<FuturesUnordered<_>>()
            .into_stream()
            .collect()
            .await)
    } else {
        eprintln!("WARN: could not find table for {url_str}");
        Ok(vec![])
    }
}

pub async fn fetch(format: Format) -> anyhow::Result<Vec<(Card, Option<Metadata>)>> {
    urls_from_format(format)?
        .map(|url| async move {
            let url_str = url.to_string();
            let s = scrape(url).await.map(stream::iter);
            println!("{url_str} scraped");
            s
        })
        .into_iter()
        .collect::<FuturesUnordered<_>>()
        .into_stream()
        .try_flatten()
        .map_ok(|(card, meta)| (card, Some(meta)))
        .try_collect()
        .await
}
