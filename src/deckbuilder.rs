use std::{collections::HashMap, num::NonZeroU8, pin::pin};

use anyhow::bail;
use reqwest::Url;
use scraper::{Html, Selector};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

use crate::collection::Collection;

fn is_basic_land(name: &str) -> bool {
    matches!(name, "Plains" | "Island" | "Swamp" | "Mountain" | "Forest")
}

struct Entry {
    owned: u8,
    count: u8,
}

struct Decklist {
    decklist: HashMap<String, Entry>,
    collection: Collection,
}

impl Decklist {
    fn new(collection: Collection) -> Self {
        Self {
            decklist: Default::default(),
            collection,
        }
    }

    fn add(&mut self, name: &str, count: u8) {
        let owned = if is_basic_land(name) {
            count
        } else {
            self.collection.get(name.into()).len() as u8
        };

        self.decklist
            .entry(name.to_owned())
            .and_modify(|v| v.count += count)
            .or_insert(Entry {
                owned: owned as u8,
                count,
            });
    }

    fn display(&self) {
        let mut as_vec = self.decklist.iter().collect::<Vec<_>>();
        as_vec.sort_by_key(|(name, _)| *name);

        for (name, Entry { owned, count }) in &as_vec {
            println!(
                "{owned}/{count}\t{}\t{name}",
                match u8::saturating_sub(*count, *owned) {
                    0 => "✅",
                    x if x < *count => "🟡",
                    _ => "❌",
                }
            )
        }

        println!("Wishlist missing:");
        for (name, Entry { owned, count }) in &as_vec {
            if let Some(count) = count.checked_sub(*owned).and_then(NonZeroU8::new) {
                println!("{} {name}", count);
            }
        }
    }
}

pub async fn load_from_web_page(url: Url, collection: Collection) -> anyhow::Result<()> {
    println!("Downloading list");
    let text = reqwest::get(url).await?.text().await?;
    println!("Done!");

    let doc = Html::parse_document(&text);
    let selector = Selector::parse(r#"div[class="deck_line hover_tr"]"#).unwrap();

    let mut decklist = Decklist::new(collection);

    for card in doc.select(&selector) {
        let mut line = card.text();
        let count: u8 = match line.next().map(|n| n.trim().parse()) {
            Some(Ok(c)) => c,
            Some(Err(e)) => bail!(
                "expected a number, got {}: {e:?}",
                card.text().next().unwrap()
            ),
            None => bail!("got an empty line"),
        };

        let Some(name) = line.next().map(|s| s.trim()) else {
            bail!("expected a card name");
        };

        decklist.add(name, count)
    }
    decklist.display();
    Ok(())
}

pub async fn check<R: AsyncRead>(deck: R, collection: Collection) -> anyhow::Result<()> {
    let deck = pin!(deck);
    let mut reader = BufReader::new(deck);
    let mut buf = String::new();

    let mut decklist = Decklist::new(collection);

    while {
        buf.clear();
        reader.read_line(&mut buf).await? > 0
    } {
        let buf = buf.trim();
        if matches!(buf, "" | "Deck" | "Sideboard") {
            continue;
        }
        let Some(end_count) = buf.find(|c: char| c.is_whitespace()) else {
            bail!("expected [count] [cardname] got {:?}", buf.trim());
        };
        let Ok(count) = buf[0..end_count].trim_end_matches('x').parse::<u8>() else {
            bail!("expected [count] [cardname] got {:?}", buf.trim());
        };
        let name = buf[end_count..].trim_start();

        decklist.add(name, count);
    }

    decklist.display();

    Ok(())
}
