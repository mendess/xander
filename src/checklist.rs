use std::{
    cell::{Ref, RefCell},
    cmp::Ordering,
    collections::HashMap,
    io,
    ops::Index,
    path::PathBuf,
    sync::OnceLock,
};

use anyhow::{bail, Context};
use futures_util::{stream, StreamExt, TryStreamExt};
use scryfall::{card::Color, set::SetCode, Card};
use serde::{Deserialize, Serialize};
use tokio::sync::{OnceCell, RwLock, Semaphore};
use uuid::Uuid;

use crate::{collection::Collection, staples::Metadata, PROG_NAME};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Set {
    pub code: SetCode,
    pub name: String,
}

// TODO: dedup from super::staples
async fn get_printings_cached(card: &Card) -> anyhow::Result<Vec<Set>> {
    fn cache_dir() -> &'static PathBuf {
        static CACHE_DIR: OnceLock<PathBuf> = OnceLock::new();
        CACHE_DIR.get_or_init(|| {
            let mut cache_dir = dirs::cache_dir().unwrap();
            cache_dir.push(PROG_NAME);
            cache_dir.push("printings.json");
            cache_dir
        })
    }
    static STAPLE_CACHE: OnceCell<RwLock<HashMap<Uuid, Vec<Set>>>> = OnceCell::const_new();
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

    if let Some(sets) = cache.read().await.get(&card.id) {
        return Ok(sets.clone());
    }

    let _permit = CONCURRENCY.acquire().await.unwrap();

    let printings = card
        .prints_search_uri
        .fetch_iter()
        .await?
        .into_stream()
        .and_then(|card| async move {
            Ok(Set {
                code: card.set,
                name: scryfall::Set::code(card.set.as_ref()).await?.name,
            })
        })
        .try_collect::<Vec<_>>()
        .await
        .with_context(|| format!("downloading printings of {}", card.name))?;
    let mut cache = cache.write().await;
    cache.insert(card.id, printings.clone());
    let cache = serde_json::to_vec::<HashMap<_, _>>(&*cache).unwrap();
    tokio::fs::write(cache_dir(), cache).await?;
    println!("downloaded printings of {} ", card.name);
    Ok(printings)
}

#[derive(Debug)]
pub struct ChecklistCard {
    pub card: Card,
    pub printings: Vec<Set>,
    owned_versions: RefCell<Vec<SetCode>>,
    pub metadata: Metadata,
}

impl ChecklistCard {
    pub fn owned_versions(&self) -> Ref<'_, Vec<SetCode>> {
        self.owned_versions.borrow()
    }

    pub fn add_version(&self, code: SetCode) -> usize {
        let mut v = self.owned_versions.borrow_mut();
        v.push(code);
        v.len()
    }

    pub fn remove_version(&self, code: SetCode) -> usize {
        let mut v = self.owned_versions.borrow_mut();
        v.iter()
            .position(|s| s == &code)
            .map(|index| v.remove(index));
        v.len()
    }

    fn cmp<F>(&self, other: &Self, missing: F) -> Ordering
    where
        F: Fn(&ChecklistCard) -> usize,
    {
        fn colors_of(card: &Card) -> Option<&[Color]> {
            card.colors.as_deref().or_else(|| {
                card.card_faces
                    .as_ref()
                    .and_then(|faces| faces.get(0))
                    .and_then(|c| c.colors.as_deref())
            })
        }
        ((missing(self) as f32 * self.metadata.percent_in_decks)
            .total_cmp(&(missing(other) as f32 * other.metadata.percent_in_decks))
            .reverse())
        .then_with(|| {
            self.metadata
                .percent_in_decks
                .total_cmp(&other.metadata.percent_in_decks)
                .reverse()
        })
        .then_with(|| colors_of(&self.card).cmp(&colors_of(&other.card)))
        .then_with(|| self.card.name.cmp(&other.card.name))
    }

    pub fn cmp_using_collected(&self, other: &Self) -> Ordering {
        fn missing(card: &ChecklistCard) -> usize {
            (card.metadata.num_copies as usize).saturating_sub(card.owned_versions().len())
        }
        self.cmp(other, missing)
    }

    pub fn cmp_ignoring_collected(&self, other: &Self) -> Ordering {
        self.cmp(other, |c| c.metadata.num_copies as _)
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
                owned_versions: versions,
                printings: get_printings_cached(&card).await?,
                card,
                metadata: metadata.unwrap_or(DEFAULT_METADATA),
            })
        })
        .buffer_unordered(8)
        .try_collect::<Vec<_>>()
        .await?;

        checklist.sort_by(|card_a, card_b| card_a.cmp_using_collected(card_b));

        Ok(Checklist(checklist))
    }

    pub fn iter(&self) -> core::slice::Iter<'_, ChecklistCard> {
        self.0.iter()
    }

    pub fn ignoring_collection(&self) -> Vec<&ChecklistCard> {
        let mut cards = self.0.iter().collect::<Vec<_>>();
        cards.sort_by(|a, b| a.cmp_ignoring_collected(b));
        cards
    }
}
