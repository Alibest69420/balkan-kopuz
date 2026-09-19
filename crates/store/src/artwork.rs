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
        let call = Link::global(cx).call(move |api| async move {
            api.artwork(ArtworkRequest {
                target: source.target,
                hq: source.hq,
            })
            .await
        });
        async move {
            match call.await {
                Ok(data) => Ok(Arc::new(Image::from_bytes(
                    image_format(&data.content_type),
                    data.bytes,
                ))),
                Err(error) => Err(ImageCacheError::Asset(error.to_string().into())),
            }
        }
    }
}

/// An `img()` source for a row's artwork; `hq` asks for the full-size picture rather than a thumbnail.
pub fn artwork_source(artwork: &ArtworkRef, hq: bool) -> ImageSource {
    let key = ArtworkKey::new(artwork, hq);
    ImageSource::Custom(Arc::new(
        move |window: &mut Window, cx: &mut App| match window.use_asset::<ArtworkBytes>(&key, cx) {
            Some(Ok(image)) => image.use_render_image(window, cx).map(Ok),
            Some(Err(error)) => Some(Err(error)),
            None => None,
        },
    ))
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
