//! DIDL-Lite, the XML a ContentDirectory answers `Browse` and `Search` with.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resource {
    pub url: String,
    /// `http-get:*:audio/flac:DLNA.ORG_PN=...`; the third field is the MIME type.
    pub protocol_info: String,
    pub duration_ms: Option<u64>,
    pub size: Option<u64>,
    /// Bytes per second, as UPnP counts it.
    pub bitrate: Option<u32>,
    pub sample_rate: Option<u32>,
}

impl Resource {
    pub fn mime(&self) -> &str {
        self.protocol_info.split(':').nth(2).unwrap_or_default()
    }

    /// The server converts the file on the fly for this resource (`DLNA.ORG_CI=1`).
    fn transcoded(&self) -> bool {
        self.protocol_info.contains("DLNA.ORG_CI=1")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub title: String,
    pub class: String,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub creator: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub track_number: Option<u32>,
    pub date: Option<String>,
    pub album_art: Option<String>,
    pub resources: Vec<Resource>,
}

impl Item {
    pub fn is_audio(&self) -> bool {
        self.class.starts_with("object.item.audioItem")
    }

    /// The resource to play: an untranscoded audio stream over HTTP if there
    /// is one, else the first audio stream offered.
    pub fn stream(&self) -> Option<&Resource> {
        let playable = |res: &&Resource| {
            res.protocol_info.starts_with("http-get:") && res.mime().starts_with("audio/")
        };
        self.resources
            .iter()
            .filter(playable)
            .find(|res| !res.transcoded())
            .or_else(|| self.resources.iter().find(playable))
    }

    pub fn year(&self) -> u16 {
        self.date
            .as_deref()
            .and_then(|date| date.get(..4))
            .and_then(|year| year.parse().ok())
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Container {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Didl {
    pub items: Vec<Item>,
    pub containers: Vec<Container>,
}

/// Which object the element being read belongs to.
enum Open {
    Item(Box<Item>),
    Container(Container),
}

pub fn parse(xml: &str) -> Result<Didl, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut didl = Didl::default();
    let mut open: Option<Open> = None;
    // The element whose text is being read, with its attributes.
    let mut field: Option<(String, Vec<(String, String)>)> = None;
    let mut text = String::new();
    loop {
        match reader.read_event() {
            Err(error) => return Err(format!("DIDL-Lite: {error}")),
            Ok(Event::Eof) => return Ok(didl),
            Ok(Event::Start(start)) => {
                let name = local(&start);
                match name.as_str() {
                    "item" => {
                        open = Some(Open::Item(Box::new(Item {
                            id: attribute(&start, "id").unwrap_or_default(),
                            ..Item::default()
                        })));
                    }
                    "container" => {
                        open = Some(Open::Container(Container {
                            id: attribute(&start, "id").unwrap_or_default(),
                            ..Container::default()
                        }));
                    }
                    _ if open.is_some() => {
                        field = Some((name, attributes(&start)));
                        text.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(chunk)) => {
                if field.is_some() {
                    text.push_str(&chunk.unescape().map_err(|e| format!("DIDL-Lite: {e}"))?);
                }
            }
            Ok(Event::CData(chunk)) => {
                if field.is_some() {
                    text.push_str(&String::from_utf8_lossy(&chunk));
                }
            }
            Ok(Event::End(end)) => {
                let name = String::from_utf8_lossy(end.local_name().as_ref()).into_owned();
                match (name.as_str(), open.take()) {
                    ("item", Some(Open::Item(item))) => didl.items.push(*item),
                    ("container", Some(Open::Container(container))) => {
                        didl.containers.push(container)
                    }
                    (_, still) => {
                        open = still;
                        if let (Some((field_name, attrs)), Some(object)) =
                            (field.take_if(|(open, _)| *open == name), open.as_mut())
                        {
                            set(object, &field_name, &attrs, text.trim().to_string());
                        }
                    }
                }
            }
            Ok(_) => {}
        }
    }
}

fn set(object: &mut Open, name: &str, attrs: &[(String, String)], value: String) {
    let attr = |key: &str| {
        attrs
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    };
    let some = |value: String| (!value.is_empty()).then_some(value);
    match object {
        Open::Container(container) => {
            if name == "title" {
                container.title = value;
            }
        }
        Open::Item(item) => match name {
            "title" => item.title = value,
            "class" => item.class = value,
            "creator" => item.creator = item.creator.take().or(some(value)),
            "album" => item.album = item.album.take().or(some(value)),
            "genre" => item.genre = item.genre.take().or(some(value)),
            "date" => item.date = item.date.take().or(some(value)),
            "albumArtURI" => item.album_art = item.album_art.take().or(some(value)),
            "originalTrackNumber" => item.track_number = value.parse().ok(),
            "artist" => match attr("role") {
                Some(role) if role.eq_ignore_ascii_case("AlbumArtist") => {
                    item.album_artist = item.album_artist.take().or(some(value));
                }
                Some(role)
                    if !role.eq_ignore_ascii_case("Performer")
                        && !role.eq_ignore_ascii_case("Artist") => {}
                _ => item.artist = item.artist.take().or(some(value)),
            },
            "res" if !value.is_empty() => item.resources.push(Resource {
                url: value,
                protocol_info: attr("protocolInfo").unwrap_or_default().to_string(),
                duration_ms: attr("duration").and_then(duration_ms),
                size: attr("size").and_then(|size| size.parse().ok()),
                bitrate: attr("bitrate").and_then(|rate| rate.parse().ok()),
                sample_rate: attr("sampleFrequency").and_then(|rate| rate.parse().ok()),
            }),
            _ => {}
        },
    }
}

fn local(start: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(start.local_name().as_ref()).into_owned()
}

fn attributes(start: &BytesStart<'_>) -> Vec<(String, String)> {
    start
        .attributes()
        .flatten()
        .map(|attr| {
            (
                String::from_utf8_lossy(attr.key.local_name().as_ref()).into_owned(),
                attr.unescape_value()
                    .map(|value| value.into_owned())
                    .unwrap_or_default(),
            )
        })
        .collect()
}

fn attribute(start: &BytesStart<'_>, key: &str) -> Option<String> {
    attributes(start)
        .into_iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value)
}

/// `H+:MM:SS[.F+]`, the UPnP duration format.
pub fn duration_ms(text: &str) -> Option<u64> {
    let mut parts = text.trim().split(':');
    let hours: u64 = parts.next()?.parse().ok()?;
    let minutes: u64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    Some((hours * 3600 + minutes * 60) * 1000 + (seconds * 1000.0).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIDLNA: &str = r#"<DIDL-Lite xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/" xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" xmlns:dlna="urn:schemas-dlna-org:metadata-1-0/">
<container id="1$4" parentID="1" restricted="1" searchable="1" childCount="3"><dc:title>Albums</dc:title><upnp:class>object.container.storageFolder</upnp:class></container>
<item id="64$0$1" parentID="64$0" restricted="1"><dc:title>Music Is Math</dc:title><upnp:class>object.item.audioItem.musicTrack</upnp:class><dc:creator>Boards of Canada</dc:creator><upnp:artist>Boards of Canada</upnp:artist><upnp:artist role="AlbumArtist">Boards of Canada</upnp:artist><upnp:album>Geogaddi</upnp:album><upnp:genre>Electronic</upnp:genre><upnp:originalTrackNumber>2</upnp:originalTrackNumber><dc:date>2002-02-18</dc:date><upnp:albumArtURI dlna:profileID="JPEG_TN">http://10.0.0.2:8200/AlbumArt/1-22.jpg</upnp:albumArtURI><res size="33554432" duration="0:05:21.453" bitrate="104857" sampleFrequency="44100" nrAudioChannels="2" protocolInfo="http-get:*:audio/x-flac:*">http://10.0.0.2:8200/MediaItems/22.flac</res></item>
<item id="64$0$2" parentID="64$0" restricted="1"><dc:title>Cover.jpg</dc:title><upnp:class>object.item.imageItem.photo</upnp:class><res protocolInfo="http-get:*:image/jpeg:*">http://10.0.0.2:8200/MediaItems/23.jpg</res></item>
</DIDL-Lite>"#;

    #[test]
    fn a_minidlna_listing_reads_into_items_and_containers() {
        let didl = parse(MINIDLNA).unwrap();
        assert_eq!(
            didl.containers,
            [Container {
                id: "1$4".into(),
                title: "Albums".into()
            }]
        );
        assert_eq!(didl.items.len(), 2);
        let track = &didl.items[0];
        assert!(track.is_audio());
        assert!(!didl.items[1].is_audio());
        assert_eq!(track.id, "64$0$1");
        assert_eq!(track.title, "Music Is Math");
        assert_eq!(track.artist.as_deref(), Some("Boards of Canada"));
        assert_eq!(track.album_artist.as_deref(), Some("Boards of Canada"));
        assert_eq!(track.album.as_deref(), Some("Geogaddi"));
        assert_eq!(track.track_number, Some(2));
        assert_eq!(track.year(), 2002);
        assert_eq!(
            track.album_art.as_deref(),
            Some("http://10.0.0.2:8200/AlbumArt/1-22.jpg")
        );
        let stream = track.stream().unwrap();
        assert_eq!(stream.url, "http://10.0.0.2:8200/MediaItems/22.flac");
        assert_eq!(stream.mime(), "audio/x-flac");
        assert_eq!(stream.duration_ms, Some(321_453));
        assert_eq!(stream.sample_rate, Some(44_100));
    }

    #[test]
    fn the_original_file_wins_over_a_transcode() {
        let xml = r#"<DIDL-Lite><item id="a"><dc:title>T</dc:title><upnp:class>object.item.audioItem</upnp:class>
<res protocolInfo="http-get:*:audio/mpeg:DLNA.ORG_PN=MP3;DLNA.ORG_CI=1">http://s/a.mp3</res>
<res protocolInfo="http-get:*:audio/flac:DLNA.ORG_CI=0">http://s/a.flac</res>
<res protocolInfo="rtsp-rtp-udp:*:audio/flac:*">rtsp://s/a</res>
</item></DIDL-Lite>"#;
        let didl = parse(xml).unwrap();
        assert_eq!(didl.items[0].stream().unwrap().url, "http://s/a.flac");
    }

    #[test]
    fn escaped_text_and_composer_credits_are_handled() {
        let xml = r#"<DIDL-Lite><item id="b"><dc:title>Rock &amp; Roll</dc:title><upnp:class>object.item.audioItem.musicTrack</upnp:class><upnp:artist role="Composer">Someone</upnp:artist><upnp:artist role="Performer">Band</upnp:artist></item></DIDL-Lite>"#;
        let item = &parse(xml).unwrap().items[0];
        assert_eq!(item.title, "Rock & Roll");
        assert_eq!(item.artist.as_deref(), Some("Band"));
    }

    #[test]
    fn durations_follow_the_upnp_format() {
        assert_eq!(duration_ms("1:02:03.5"), Some(3_723_500));
        assert_eq!(duration_ms("0:00:07"), Some(7_000));
        assert_eq!(duration_ms("7"), None);
    }
}
