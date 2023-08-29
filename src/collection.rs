use std::{collections::HashMap, io, path::PathBuf, sync::OnceLock};

use anyhow::bail;
use scryfall::set::SetCode;
use tokio::io::AsyncWriteExt;

use crate::{PROG_NAME, card_name::{CardName, CName}};

type Versions = Vec<SetCode>;

pub struct Collection(pub HashMap<CardName, Versions>);

impl Collection {
    pub fn get(&'_ self, name: &CName) -> &'_ [SetCode] {
        self.0.get(name.trimming_double_faced()).map(|v| v.as_slice()).unwrap_or(&[][..])
    }
}

fn collection_file() -> &'static PathBuf {
    static COLLECTION_FILE: OnceLock<PathBuf> = OnceLock::new();
    COLLECTION_FILE.get_or_init(|| {
        let mut path = dirs::config_dir().unwrap();
        path.push(PROG_NAME);
        path.push("collection.json");
        path
    })
}

pub async fn del_from_collection(card: CardName, to_del: SetCode) -> anyhow::Result<()> {
    let Collection(mut collection) = load().await?;
    let card = card.as_slice().trimming_double_faced();
    if let Some(versions) = collection.get_mut(card) {
        if let Some(to_del) = versions.iter().position(|s| s == &to_del) {
            versions.remove(to_del);
            store(&collection).await?;
        }
    }

    Ok(())
}

pub async fn add_to_collection(card: CardName, new_version: SetCode) -> anyhow::Result<()> {
    let Collection(mut collection) = load().await?;
    let card = card.trimming_double_faced();
    collection
        .entry(card)
        .and_modify(|versions| versions.push(new_version))
        .or_insert_with(|| vec![new_version]);

    store(&collection).await?;

    Ok(())
}

async fn store(collection: &HashMap<CardName, Versions>) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(collection_file().parent().unwrap()).await?;
    tokio::fs::File::create(&collection_file())
        .await?
        .write_all(&serde_json::to_vec(&collection).unwrap())
        .await?;

    Ok(())
}

pub async fn load() -> anyhow::Result<Collection> {
    let collection = match tokio::fs::read(collection_file()).await {
        Ok(collection) => serde_json::from_slice(&collection).unwrap(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let default = HashMap::default();
            store(&default).await?;
            default
        }
        Err(e) => bail!(e),
    };
    Ok(Collection(collection))
}
