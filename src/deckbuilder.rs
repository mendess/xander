use std::{collections::HashMap, pin::pin};

use anyhow::bail;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

use crate::collection::Collection;

fn is_basic_land(name: &str) -> bool {
    matches!(name, "Plains" | "Island" | "Swamp" | "Mountain" | "Forest")
}

pub async fn check<R: AsyncRead>(deck: R, collection: Collection) -> anyhow::Result<()> {
    let deck = pin!(deck);
    let mut reader = BufReader::new(deck);
    let mut buf = String::new();

    let mut decklist = HashMap::<_, (usize, _)>::new();
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

        let owned = if is_basic_land(name) {
            count.into()
        } else {
            collection.get(name).len()
        };

        decklist
            .entry(name.to_owned())
            .and_modify(|v| v.0 += owned)
            .or_insert((owned, count));
    }

    for (name, (owned, count)) in decklist {
        let count = count.into();
        println!(
            "{owned}/{count}\t{}\t{name}",
            match usize::saturating_sub(count, owned) {
                0 => "✅",
                x if x < count => "🟡",
                _ => "❌",
            }
        )
    }

    Ok(())
}
