use std::{
    cell::{Ref, RefCell},
    collections::HashMap,
    io,
    ops::Index,
    path::PathBuf,
    sync::OnceLock,
};

use anyhow::{bail, Context};
use futures_util::{stream, StreamExt, TryStreamExt};
use scryfall::{card::Color, set::SetCode, Card};
use tokio::sync::{OnceCell, RwLock, Semaphore};
use uuid::Uuid;

use crate::{collection::Collection, staples::Metadata, PROG_NAME};

// TODO: dedup from super::staples
async fn get_printings_cached(card: &Card) -> anyhow::Result<Vec<SetCode>> {
    fn cache_dir() -> &'static PathBuf {
        static CACHE_DIR: OnceLock<PathBuf> = OnceLock::new();
        CACHE_DIR.get_or_init(|| {
            let mut cache_dir = dirs::cache_dir().unwrap();
            cache_dir.push(PROG_NAME);
            cache_dir.push("printings.json");
            cache_dir
        })
    }
    static STAPLE_CACHE: OnceCell<RwLock<HashMap<Uuid, Vec<SetCode>>>> = OnceCell::const_new();
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

    if let Some(card) = cache.read().await.get(&card.id) {
        return Ok(card.clone());
    }

    let _permit = CONCURRENCY.acquire().await.unwrap();

    let printings = card
        .prints_search_uri
        .fetch_iter()
        .await?
        .into_stream()
        .map_ok(|card| card.set)
        .try_collect::<Vec<_>>()
        .await
        .with_context(|| format!("downloading printings of {}", card.name))?;
    let mut cache = cache.write().await;
    cache.insert(card.id, printings.clone());
    let cache = serde_json::to_vec::<HashMap<_, _>>(&*cache).unwrap();
    tokio::fs::write(cache_dir(), cache).await?;
    println!("{} printings downloading", card.name);
    Ok(printings)
}

pub struct ChecklistCard {
    pub card: Card,
    pub printings: Vec<SetCode>,
    versions: RefCell<Vec<SetCode>>,
    pub metadata: Metadata,
}

impl ChecklistCard {
    pub fn versions(&self) -> Ref<'_, Vec<SetCode>> {
        self.versions.borrow()
    }

    pub fn add_version(&self, code: SetCode) -> usize {
        let mut v = self.versions.borrow_mut();
        v.push(code);
        v.len()
    }

    pub fn remove_version(&self, code: SetCode) -> usize {
        let mut v = self.versions.borrow_mut();
        v.iter()
            .position(|s| s == &code)
            .map(|index| v.remove(index));
        v.len()
    }
}

pub struct Checklist(Vec<ChecklistCard>);

impl IntoIterator for Checklist {
    type Item = ChecklistCard;
    type IntoIter = <Vec<Self::Item> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'c> IntoIterator for &'c Checklist {
    type Item = &'c ChecklistCard;
    type IntoIter = <&'c Vec<ChecklistCard> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl Index<usize> for Checklist {
    type Output = ChecklistCard;
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl Checklist {
    pub async fn new(
        staples: Vec<(Card, Option<Metadata>)>,
        collection: Collection,
    ) -> anyhow::Result<Self> {
        let mut checklist = stream::iter(
            staples
                .into_iter()
                .filter(|(card, _)| {
                    card.type_line.is_none()
                        || card
                            .type_line
                            .as_ref()
                            .is_some_and(|line| !line.contains("Basic"))
                })
                .map(|card| (RefCell::new(collection.get(&card.0.name).into()), card)),
        )
        .map(|(versions, (card, metadata))| async move {
            const DEFAULT_METADATA: Metadata = Metadata {
                num_copies: 4,
                percent_in_decks: 100.,
            };
            anyhow::Ok(ChecklistCard {
                versions,
                printings: get_printings_cached(&card).await?,
                card,
                metadata: metadata.unwrap_or(DEFAULT_METADATA),
            })
        })
        .buffer_unordered(8)
        .try_collect::<Vec<_>>()
        .await?;

        checklist.sort_by(|card_a, card_b| {
            fn colors_of(card: &Card) -> Option<&[Color]> {
                card.colors.as_deref().or_else(|| {
                    card.card_faces
                        .as_ref()
                        .and_then(|faces| faces.get(0))
                        .and_then(|c| c.colors.as_deref())
                })
            }
            fn missing(card: &ChecklistCard) -> usize {
                (card.metadata.num_copies as usize).saturating_sub(card.versions().len())
            }
            ((missing(card_a) as f32 * card_a.metadata.percent_in_decks)
                .total_cmp(&(missing(card_b) as f32 * card_b.metadata.percent_in_decks))
                .reverse())
            .then_with(|| {
                card_a
                    .metadata
                    .percent_in_decks
                    .total_cmp(&card_b.metadata.percent_in_decks)
                    .reverse()
            })
            .then_with(|| colors_of(&card_a.card).cmp(&colors_of(&card_b.card)))
            .then_with(|| card_a.card.name.cmp(&card_b.card.name))
        });
        Ok(Checklist(checklist))
    }

    pub fn iter(&self) -> core::slice::Iter<'_, ChecklistCard> {
        self.0.iter()
    }
}
