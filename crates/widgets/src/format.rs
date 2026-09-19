use api::schema::Text;
use gpui_kit::SharedString;

/// A translated string, ready to hand to an element.
pub fn t(key: &str) -> SharedString {
    i18n::t(key).into()
}

/// A translated string with Fluent arguments.
pub fn t_with(key: &str, args: &[(&str, String)]) -> SharedString {
    i18n::t_with(key, args).into()
}

/// Daemon-supplied text: a key is translated, a literal is shown as it came.
pub fn text(value: &Text) -> SharedString {
    match value {
        Text::Key(key) => t(key),
        Text::Literal(literal) => literal.clone().into(),
    }
}

/// `m:ss`, or `h:mm:ss` past an hour.
pub fn duration(ms: u64) -> SharedString {
    let secs = ms / 1000;
    let (hours, minutes, seconds) = (secs / 3600, (secs / 60) % 60, secs % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}").into()
    } else {
        format!("{minutes}:{seconds:02}").into()
    }
}

/// A listing's total length, as "1 hr 12 min" reads better than a clock there.
pub fn total_duration(ms: u64) -> SharedString {
    let minutes = ms / 60_000;
    if minutes >= 60 {
        format!("{} hr {} min", minutes / 60, minutes % 60).into()
    } else {
        format!("{minutes} min").into()
    }
}
