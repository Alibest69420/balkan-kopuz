use std::sync::Arc;

use api::{ArtworkRef, ArtworkRequest, ArtworkTarget};
use gpui_kit::{App, Asset, Image, ImageCacheError, ImageFormat, ImageSource, Window};
use link::Link;

/// One cover at one size; `version` changes exactly when the picture would, so a hit caches forever.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArtworkKey {
    pub target: ArtworkTarget,
    pub version: u64,
    pub hq: bool,
}

impl ArtworkKey {
    pub fn new(artwork: &ArtworkRef, hq: bool) -> Self {
        Self {
            target: artwork.target.clone(),
            version: artwork.version,
            hq,
        }
    }
}

/// Fetches cover bytes through the daemon; GPUI's asset cache keys, dedupes and decodes them.
pub enum ArtworkBytes {}

impl Asset for ArtworkBytes {
    type Source = ArtworkKey;
    type Output = Result<Arc<Image>, ImageCacheError>;

    fn load(
        source: Self::Source,
        cx: &mut App,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        let hq = source.hq;
        let call = Link::global(cx).call(move |api| async move {
            api.artwork(ArtworkRequest {
                target: source.target,
                hq: source.hq,
            })
            .await
        });
        async move {
            let span = tracing::debug_span!("artwork.load", hq);
            let _entered = span.enter();
            match call.await {
                Ok(data) => {
                    // Counted here: this is the one place a cover enters the process.
                    let (width, height) = probe_size(&data.bytes).unwrap_or((0, 0));
                    profile::artwork::loaded(data.bytes.len(), width, height);
                    Ok(Arc::new(Image::from_bytes(
                        image_format(&data.content_type),
                        data.bytes,
                    )))
                }
                Err(error) => Err(ImageCacheError::Asset(error.to_string().into())),
            }
        }
    }
}

/// The decoded dimensions, read from the header rather than by decoding the whole picture.
fn probe_size(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

/// An `img()` source for a row's artwork; `hq` asks for the full-size picture rather than a thumbnail.
pub fn artwork_source(artwork: &ArtworkRef, hq: bool) -> ImageSource {
    let key = ArtworkKey::new(artwork, hq);
    ImageSource::Custom(Arc::new(move |window: &mut Window, cx: &mut App| {
        match window.use_asset::<ArtworkBytes>(&key, cx) {
            Some(Ok(image)) => {
                // Touched on every frame it is drawn, which is what keeps it out of the sweep.
                Residents::touch(&key, &image, cx);
                image.use_render_image(window, cx).map(Ok)
            }
            Some(Err(error)) => Some(Err(error)),
            None => None,
        }
    }))
}

/// How many decoded covers to keep. A screenful is a few dozen; this is several screens of scrollback.
const BUDGET: usize = 256;

/// `KOPUZ_ARTWORK_BUDGET` overrides it, which is how the sweep gets tested without scrolling a library.
fn budget() -> usize {
    std::env::var("KOPUZ_ARTWORK_BUDGET")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(BUDGET)
}

/// The covers currently decoded, so the oldest can be released once the budget is passed.
///
/// GPUI caches an image in three places at once and frees none of them on its own: the fetched
/// bytes in the asset cache, the decoded RGBA in the image cache, and the texture in the window's
/// sprite atlas. Dropping the element does not drop any of them.
#[derive(Default)]
pub struct Residents {
    live: std::collections::HashMap<ArtworkKey, Resident>,
    clock: u64,
}

struct Resident {
    image: Arc<Image>,
    used: u64,
}

impl gpui_kit::Global for Residents {}

impl Residents {
    /// Records that `key` was drawn this frame.
    fn touch(key: &ArtworkKey, image: &Arc<Image>, cx: &mut App) {
        let residents = cx.default_global::<Self>();
        residents.clock += 1;
        let clock = residents.clock;
        match residents.live.get_mut(key) {
            Some(resident) => resident.used = clock,
            None => {
                residents.live.insert(
                    key.clone(),
                    Resident {
                        image: image.clone(),
                        used: clock,
                    },
                );
            }
        }
    }

    /// Releases the least recently drawn covers until the budget is met.
    pub fn sweep(window: &mut Window, cx: &mut App) {
        let over = cx
            .default_global::<Self>()
            .live
            .len()
            .saturating_sub(budget());
        if over == 0 {
            return;
        }
        let residents = cx.default_global::<Self>();
        let mut ages: Vec<(u64, ArtworkKey)> = residents
            .live
            .iter()
            .map(|(key, resident)| (resident.used, key.clone()))
            .collect();
        ages.sort_unstable_by_key(|(used, _)| *used);
        let doomed: Vec<ArtworkKey> = ages.into_iter().take(over).map(|(_, key)| key).collect();
        for key in doomed {
            let Some(resident) = cx.default_global::<Self>().live.remove(&key) else {
                continue;
            };
            // The atlas texture goes first; it is the only one that needs the window.
            if let Some(rendered) = resident.image.clone().get_render_image(window, cx) {
                cx.drop_image(rendered, Some(window));
            }
            resident.image.remove_asset(cx);
            cx.remove_asset::<ArtworkBytes>(&key);
            profile::artwork::released();
        }
    }
}

fn image_format(content_type: &str) -> ImageFormat {
    match content_type {
        "image/png" => ImageFormat::Png,
        "image/webp" => ImageFormat::Webp,
        "image/gif" => ImageFormat::Gif,
        "image/bmp" => ImageFormat::Bmp,
        "image/svg+xml" => ImageFormat::Svg,
        _ => ImageFormat::Jpeg,
    }
}
