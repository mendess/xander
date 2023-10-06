use anyhow::Context;
use futures_util::StreamExt;
use scryfall::Card;
use std::future::Future;
use tokio::{fs::File, io::AsyncWriteExt};

pub fn show(card: &Card) -> Option<impl Future<Output = anyhow::Result<()>>> {
    let uri = if let Some(large) = card.image_uris.get("large") {
        Some(large)
    } else if let Some(faces) = &card.card_faces {
        faces
            .iter()
            .find_map(|face| face.image_uris.as_ref().and_then(|u| u.get("large")))
    } else {
        None
    }
    .cloned();

    let Some(uri) = uri else { return None };
    Some(async move {
        let (file, path) = tempfile::Builder::new()
            .suffix(".png")
            .tempfile()
            .context("failed to create tempfile")?
            .into_parts();
        let mut file = File::from_std(file);
        let mut bytes = reqwest::get(uri.clone())
            .await
            .context("failed to fetch card image")?
            .bytes_stream();
        while let Some(b) = bytes.next().await {
            file.write_all(&b.context("failed to download byte chunk")?)
                .await
                .context("failed to write by chunk")?
        }
        file.flush().await.context("failed to flush")?;

        tokio::task::spawn_blocking(move || open::that(&path))
            .await?
            .context("failed to open image")?;

        Ok(())
    })
}
