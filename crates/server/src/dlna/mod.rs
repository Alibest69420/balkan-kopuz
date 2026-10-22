//! A UPnP AV (DLNA) media server client: finding servers on the LAN over
//! SSDP, reading a server's device description, and listing its
//! ContentDirectory with `Browse` and `Search` over SOAP.
//!
//! A server is configured by its description URL (what SSDP's `LOCATION`
//! header points at), by its name or host, or not at all, which takes the
//! first server that answers.

pub mod didl;

use std::time::Duration;

use quick_xml::Reader;
use quick_xml::events::Event;
use reqwest::Url;

use crate::source::SourceError;

const SSDP_ADDRESS: &str = "239.255.255.250:1900";
const MEDIA_SERVER: &str = "urn:schemas-upnp-org:device:MediaServer:1";
const CONTENT_DIRECTORY: &str = "urn:schemas-upnp-org:service:ContentDirectory:";
/// How long to wait for SSDP answers; servers reply within `MX` seconds.
const DISCOVERY_WINDOW: Duration = Duration::from_secs(3);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// A media server's ContentDirectory, as its description names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub name: String,
    pub udn: String,
    pub location: String,
    pub control_url: String,
    /// `urn:schemas-upnp-org:service:ContentDirectory:N`, echoed in each action.
    pub service_type: String,
}

/// One answer page: the objects, and how many the whole listing holds.
#[derive(Debug, Clone, Default)]
pub struct Page {
    pub didl: didl::Didl,
    pub returned: u32,
    pub total: u32,
}

pub fn client() -> Result<reqwest::Client, SourceError> {
    utils::http::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|error| SourceError::Backend(format!("DLNA HTTP client: {error}")))
}

/// Ask the LAN for media servers and return their description URLs.
pub async fn discover() -> Result<Vec<String>, SourceError> {
    let socket = tokio::net::UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|error| SourceError::Backend(format!("SSDP socket: {error}")))?;
    let search = format!(
        "M-SEARCH * HTTP/1.1\r\nHOST: {SSDP_ADDRESS}\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\nST: {MEDIA_SERVER}\r\n\r\n"
    );
    // UDP drops packets; a second probe is what UPnP control points send too.
    for _ in 0..2 {
        socket
            .send_to(search.as_bytes(), SSDP_ADDRESS)
            .await
            .map_err(|_| SourceError::Connectivity)?;
    }
    let deadline = tokio::time::Instant::now() + DISCOVERY_WINDOW;
    let mut found = Vec::new();
    let mut buffer = [0u8; 2048];
    while let Ok(Ok((len, _))) =
        tokio::time::timeout_at(deadline, socket.recv_from(&mut buffer)).await
    {
        if let Some(location) = location_header(&String::from_utf8_lossy(&buffer[..len]))
            && !found.contains(&location)
        {
            found.push(location);
        }
    }
    Ok(found)
}

fn location_header(response: &str) -> Option<String> {
    response.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("location")
            .then(|| value.trim().to_string())
            .filter(|value| value.starts_with("http"))
    })
}

/// Find the configured server: a description URL is read directly, anything
/// else is matched against what discovery turns up.
pub async fn locate(client: &reqwest::Client, configured: &str) -> Result<Device, SourceError> {
    let configured = configured.trim();
    if configured.starts_with("http://") || configured.starts_with("https://") {
        return describe(client, configured).await;
    }
    for location in discover().await? {
        let device = match describe(client, &location).await {
            Ok(device) => device,
            Err(error) => {
                tracing::debug!(%location, %error, "skipping unreadable DLNA server");
                continue;
            }
        };
        let host = Url::parse(&location)
            .ok()
            .and_then(|url| url.host_str().map(str::to_string));
        if configured.is_empty()
            || device.name.eq_ignore_ascii_case(configured)
            || host.as_deref() == Some(configured)
        {
            return Ok(device);
        }
    }
    Err(SourceError::Connectivity)
}

pub async fn describe(client: &reqwest::Client, location: &str) -> Result<Device, SourceError> {
    let body = client
        .get(location)
        .send()
        .await
        .map_err(|_| SourceError::Connectivity)?
        .error_for_status()
        .map_err(|error| SourceError::Backend(format!("DLNA description: {error}")))?
        .text()
        .await
        .map_err(|_| SourceError::Connectivity)?;
    parse_description(location, &body)
}

fn parse_description(location: &str, xml: &str) -> Result<Device, SourceError> {
    let fields = xml_fields(xml)?;
    let field = |name: &str| {
        fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    };
    // Each <service> lists its type before its URLs, so pair them in order.
    let mut service_type = None;
    let mut control_url = None;
    for (key, value) in &fields {
        match key.as_str() {
            "serviceType" => service_type = Some(value.clone()),
            "controlURL"
                if service_type
                    .as_deref()
                    .is_some_and(|kind| kind.starts_with(CONTENT_DIRECTORY)) =>
            {
                control_url = Some(value.clone());
                break;
            }
            _ => {}
        }
    }
    let (Some(service_type), Some(control_url)) = (service_type, control_url) else {
        return Err(SourceError::InvalidInput(
            "this UPnP device has no ContentDirectory; is it a media server?".into(),
        ));
    };
    let base = field("URLBase")
        .filter(|base| !base.is_empty())
        .unwrap_or_else(|| location.to_string());
    let control_url = Url::parse(&base)
        .and_then(|base| base.join(&control_url))
        .map_err(|error| SourceError::InvalidInput(format!("DLNA control URL: {error}")))?;
    Ok(Device {
        name: field("friendlyName").unwrap_or_else(|| location.to_string()),
        udn: field("UDN").unwrap_or_default(),
        location: location.to_string(),
        control_url: control_url.to_string(),
        service_type,
    })
}

/// Every element's local name and text, in document order.
fn xml_fields(xml: &str) -> Result<Vec<(String, String)>, SourceError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut fields = Vec::new();
    let mut open = None;
    loop {
        match reader.read_event() {
            Err(error) => return Err(SourceError::Backend(format!("UPnP XML: {error}"))),
            Ok(Event::Eof) => return Ok(fields),
            Ok(Event::Start(start)) => {
                open = Some(String::from_utf8_lossy(start.local_name().as_ref()).into_owned());
            }
            Ok(Event::Text(text)) => {
                if let Some(name) = open.take() {
                    let value = text
                        .unescape()
                        .map_err(|error| SourceError::Backend(format!("UPnP XML: {error}")))?;
                    fields.push((name, value.into_owned()));
                }
            }
            Ok(Event::CData(text)) => {
                if let Some(name) = open.take() {
                    fields.push((name, String::from_utf8_lossy(&text).into_owned()));
                }
            }
            Ok(Event::End(_)) => open = None,
            Ok(_) => {}
        }
    }
}

impl Device {
    pub async fn browse_children(
        &self,
        client: &reqwest::Client,
        object_id: &str,
        start: u32,
        count: u32,
    ) -> Result<Page, SourceError> {
        self.call(
            client,
            "Browse",
            &[
                ("ObjectID", object_id),
                ("BrowseFlag", "BrowseDirectChildren"),
                ("Filter", "*"),
                ("StartingIndex", &start.to_string()),
                ("RequestedCount", &count.to_string()),
                ("SortCriteria", ""),
            ],
        )
        .await
    }

    pub async fn metadata(
        &self,
        client: &reqwest::Client,
        object_id: &str,
    ) -> Result<Option<didl::Item>, SourceError> {
        let page = self
            .call(
                client,
                "Browse",
                &[
                    ("ObjectID", object_id),
                    ("BrowseFlag", "BrowseMetadata"),
                    ("Filter", "*"),
                    ("StartingIndex", "0"),
                    ("RequestedCount", "0"),
                    ("SortCriteria", ""),
                ],
            )
            .await?;
        Ok(page.didl.items.into_iter().next())
    }

    /// Every audio item under the root, for servers that support `Search`.
    pub async fn search_audio(
        &self,
        client: &reqwest::Client,
        start: u32,
        count: u32,
    ) -> Result<Page, SourceError> {
        self.call(
            client,
            "Search",
            &[
                ("ContainerID", "0"),
                (
                    "SearchCriteria",
                    "upnp:class derivedfrom \"object.item.audioItem\"",
                ),
                ("Filter", "*"),
                ("StartingIndex", &start.to_string()),
                ("RequestedCount", &count.to_string()),
                ("SortCriteria", ""),
            ],
        )
        .await
    }

    async fn call(
        &self,
        client: &reqwest::Client,
        action: &str,
        args: &[(&str, &str)],
    ) -> Result<Page, SourceError> {
        let arguments: String = args
            .iter()
            .map(|(name, value)| format!("<{name}>{}</{name}>", escape(value)))
            .collect();
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/"><s:Body><u:{action} xmlns:u="{service}">{arguments}</u:{action}></s:Body></s:Envelope>"#,
            service = self.service_type,
        );
        let response = client
            .post(&self.control_url)
            .header("Content-Type", r#"text/xml; charset="utf-8""#)
            .header("SOAPAction", format!("\"{}#{action}\"", self.service_type))
            .body(body)
            .send()
            .await
            .map_err(|_| SourceError::Connectivity)?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|_| SourceError::Connectivity)?;
        let fields = xml_fields(&text)?;
        let field = |name: &str| {
            fields
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.as_str())
        };
        if !status.is_success() {
            let code = field("errorCode").unwrap_or("?");
            let description = field("errorDescription").unwrap_or_default();
            return Err(SourceError::Backend(format!(
                "DLNA {action} failed: UPnP error {code} {description} (HTTP {status})"
            )));
        }
        let didl =
            didl::parse(field("Result").unwrap_or_default()).map_err(SourceError::Backend)?;
        let number = |name: &str| field(name).and_then(|value| value.parse().ok());
        Ok(Page {
            returned: number("NumberReturned")
                .unwrap_or((didl.items.len() + didl.containers.len()) as u32),
            total: number("TotalMatches").unwrap_or(0),
            didl,
        })
    }
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESCRIPTION: &str = r#"<?xml version="1.0"?>
<root xmlns="urn:schemas-upnp-org:device-1-0"><specVersion><major>1</major><minor>0</minor></specVersion>
<device><deviceType>urn:schemas-upnp-org:device:MediaServer:1</deviceType><friendlyName>nas: minidlna</friendlyName><UDN>uuid:4d696e69-444c-164e-9d41-001132a2c2a4</UDN>
<serviceList>
<service><serviceType>urn:schemas-upnp-org:service:ConnectionManager:1</serviceType><serviceId>urn:upnp-org:serviceId:ConnectionManager</serviceId><controlURL>/ctl/ConnectionMgr</controlURL></service>
<service><serviceType>urn:schemas-upnp-org:service:ContentDirectory:1</serviceType><serviceId>urn:upnp-org:serviceId:ContentDirectory</serviceId><controlURL>/ctl/ContentDir</controlURL><eventSubURL>/evt/ContentDir</eventSubURL></service>
</serviceList></device></root>"#;

    #[test]
    fn a_description_names_the_content_directory_control_url() {
        let device = parse_description("http://10.0.0.2:8200/rootDesc.xml", DESCRIPTION).unwrap();
        assert_eq!(device.name, "nas: minidlna");
        assert_eq!(device.udn, "uuid:4d696e69-444c-164e-9d41-001132a2c2a4");
        assert_eq!(device.control_url, "http://10.0.0.2:8200/ctl/ContentDir");
        assert_eq!(
            device.service_type,
            "urn:schemas-upnp-org:service:ContentDirectory:1"
        );
    }

    #[test]
    fn a_device_without_a_content_directory_is_refused() {
        let renderer = DESCRIPTION.replace("ContentDirectory", "RenderingControl");
        assert!(parse_description("http://10.0.0.2/d.xml", &renderer).is_err());
    }

    #[test]
    fn the_location_header_is_read_in_any_case() {
        let reply = "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=1800\r\nlocation: http://10.0.0.2:8200/rootDesc.xml\r\nST: urn:schemas-upnp-org:device:MediaServer:1\r\n\r\n";
        assert_eq!(
            location_header(reply).as_deref(),
            Some("http://10.0.0.2:8200/rootDesc.xml")
        );
        assert_eq!(location_header("HTTP/1.1 200 OK\r\n\r\n"), None);
    }

    #[test]
    fn soap_arguments_are_escaped() {
        assert_eq!(
            escape(r#"upnp:class derivedfrom "a" & <b>"#),
            "upnp:class derivedfrom &quot;a&quot; &amp; &lt;b&gt;"
        );
    }
}
