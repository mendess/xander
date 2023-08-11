pub mod goldfish;
pub mod mtgtop8;

use std::{collections::HashMap, io, path::PathBuf, sync::OnceLock};

use anyhow::bail;
use futures_util::try_join;
use scryfall::{format::Format, Card};
use tokio::{
    sync::{OnceCell, RwLock, Semaphore},
    task::LocalSet,
};

use crate::PROG_NAME;

#[derive(Debug, Clone, Copy)]
pub struct Metadata {
    pub percent_in_decks: f32,
    pub num_copies: u8,
}

impl Metadata {
    fn new(percent_in_decks: Option<f32>, num_copies: Option<u8>) -> Self {
        Self {
            percent_in_decks: percent_in_decks.unwrap_or(100.0),
            num_copies: num_copies.unwrap_or(4),
        }
    }
}

async fn get_cached(name: &str) -> anyhow::Result<Card> {
    fn fix_lotr_accented_cards(card: &str) -> &str {
        match card {
            "Lorien Revealed" => "Lórien Revealed",
            "Troll of Khazad-dum" => "Troll of Khazad-dûm",
            _ => card,
        }
    }
    let name = fix_lotr_accented_cards(name);
    fn cache_dir() -> &'static PathBuf {
        static CACHE_DIR: OnceLock<PathBuf> = OnceLock::new();
        CACHE_DIR.get_or_init(|| {
            let mut cache_dir = dirs::cache_dir().unwrap();
            cache_dir.push(PROG_NAME);
            cache_dir.push("staples.json");
            cache_dir
        })
    }
    static STAPLE_CACHE: OnceCell<RwLock<HashMap<String, Card>>> = OnceCell::const_new();
    static CONCURRENCY: Semaphore = Semaphore::const_new(8);

    let cache = STAPLE_CACHE
        .get_or_try_init(|| async {
            let cards = match tokio::fs::read(cache_dir()).await {
                Ok(cards) => cards,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    tokio::fs::create_dir_all(cache_dir().parent().unwrap()).await?;
                    tokio::fs::File::create(cache_dir()).await?;
                    vec![b'{', b'}']
                }
                Err(e) => bail!(e),
            };
            anyhow::Ok(RwLock::const_new(serde_json::from_slice(&cards)?))
        })
        .await?;

    if let Some(card) = cache.read().await.get(name) {
        return Ok(card.clone());
    }

    let _permit = CONCURRENCY.acquire().await.unwrap();

    let card = scryfall::Card::named(name).await?;
    let mut cache = cache.write().await;
    let name = match card.card_faces.as_ref().and_then(|face| face.get(0)) {
        Some(front_face) => &front_face.name,
        None => &card.name,
    };
    cache.insert(name.to_owned(), card.clone());
    let cache = serde_json::to_vec::<HashMap<_, _>>(&*cache).unwrap();
    tokio::fs::write(cache_dir(), cache).await?;
    println!("{name} downloaded");
    Ok(card)
}

pub async fn fetch(format: Format) -> anyhow::Result<Vec<(Card, Option<Metadata>)>> {
    let local_set = LocalSet::new();
    let (top8, goldfish) = try_join!(tokio::spawn(mtgtop8::fetch(format)), async {
        Ok(local_set.run_until(goldfish::fetch(format)).await)
    })?;
    let (mut top8, goldfish) = (top8.unwrap(), goldfish.unwrap());
    println!("all cards downloaded");
    println!("\ttop8: {}", top8.len());
    println!("\tgold: {}", goldfish.len());
    top8.extend(goldfish);
    println!("\ttota: {}", top8.len());
    top8.sort_unstable_by(|(card_a, meta_a), (card_b, meta_b)| {
        card_a
            .id
            .cmp(&card_b.id)
            .then_with(|| match (meta_a, meta_b) {
                (Some(m_a), Some(m_b)) => m_a
                    .percent_in_decks
                    .total_cmp(&m_b.percent_in_decks)
                    .reverse(),
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (Some(_), None) => std::cmp::Ordering::Less,
                _ => std::cmp::Ordering::Equal,
            })
    });
    top8.dedup_by(|(a, _), (b, _)| a.id == b.id);
    println!("all cards sorted {}", top8.len());

    Ok(top8)
}
