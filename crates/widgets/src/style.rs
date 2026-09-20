//! Kopuz's Tailwind look in GPUI terms; `docs/STYLING.md` holds the full class-to-call table.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::ButtonCustomVariant;
use gpui_kit::{App, Hsla, Pixels, Rems, px, rems};

/// `bg-white/5`: the raised surface a row, tile or thumb sits on.
pub const RAISED: f32 = 0.05;
/// `hover:bg-white/10`, and the resting surface of a pressed-looking control.
pub const HOVER: f32 = 0.10;
/// `hover:bg-white/20`, and `bg-white/20` on an active control.
pub const ACTIVE: f32 = 0.20;
/// `border-white/10`.
pub const BORDER: f32 = 0.10;
/// `border-white/5`, the quieter divider.
pub const BORDER_SOFT: f32 = 0.05;

/// `text-white/90`, a heading that is not quite full white.
pub const TEXT_STRONG: f32 = 0.90;
/// `text-white/60`, secondary text.
pub const TEXT_SECONDARY: f32 = 0.60;
/// `text-white/50`, a form's help line and a section heading.
pub const TEXT_MUTED: f32 = 0.50;
/// `text-white/45`, a row's subtitle.
pub const TEXT_SUBTLE: f32 = 0.45;
/// `text-white/30`, a row's album and duration columns.
pub const TEXT_DIM: f32 = 0.30;
/// `text-white/20`, a placeholder glyph.
pub const TEXT_FAINT: f32 = 0.20;
/// The `inset 0 0 0 9999px rgba(255,255,255,0.07)` a selected row is washed with.
pub const SELECTED: f32 = 0.07;
/// `color-mix(… indigo-500 12%, transparent)`: the now-playing row's tint.
pub const PLAYING: f32 = 0.12;

/// `bg-white/N`: the theme's text color at `alpha`, which is what Tailwind's
/// `--color-white` resolves to in every Kopuz theme.
pub fn white(alpha: f32, cx: &App) -> Hsla {
    cx.theme().foreground.opacity(alpha)
}

/// `bg-black/N`: the theme's background color at `alpha`, for the panels that
/// darken whatever sits behind them.
pub fn black(alpha: f32, cx: &App) -> Hsla {
    cx.theme().background.opacity(alpha)
}

/// `color-mix(in oklab, var(--color-indigo-500) N%, transparent)`: the accent as a wash.
pub fn accent(alpha: f32, cx: &App) -> Hsla {
    cx.theme().primary.opacity(alpha)
}

/// `text-slate-400`: the muted text every secondary line uses.
pub fn slate_400(cx: &App) -> Hsla {
    cx.theme().muted_foreground
}

/// `text-slate-500`: the solid mid-grey the artist and album columns use.
pub fn slate_500(cx: &App) -> Hsla {
    parse(&theme::colors(cx).surface).unwrap_or_else(|| cx.theme().muted_foreground)
}

/// `rounded` — 4px.
pub fn radius(_: &App) -> Rems {
    rems(0.25)
}

/// `rounded-lg` — 8px, the row and small-control radius.
pub fn radius_lg() -> Rems {
    rems(0.5)
}

/// `rounded-xl` — 12px, the card and panel radius.
pub fn radius_xl() -> Rems {
    rems(0.75)
}

/// `rounded-2xl` — 16px.
pub fn radius_2xl() -> Rems {
    rems(1.)
}

/// `text-[10px]`, which Tailwind cannot express on its scale.
pub fn text_10() -> Pixels {
    px(10.)
}

/// `text-[11px]`.
pub fn text_11() -> Pixels {
    px(11.)
}

/// `h-14`: the track row, and so the virtualized row height.
pub fn row_height() -> Rems {
    rems(3.5)
}

/// `w-10 h-10`: a desktop row's cover thumb.
pub fn row_cover() -> Rems {
    rems(2.5)
}

/// `w-9 h-9`: the square icon buttons in the chrome.
pub fn icon_button() -> Rems {
    rems(2.25)
}

/// Which skin is drawing. Only [`Skin::Normal`] is ported; Vaxry hangs off here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Skin {
    #[default]
    Normal,
    Vaxry,
}

impl Skin {
    /// The skin `cfg.ui_style` asks for; anything unported falls back to Normal.
    pub fn current(cx: &App) -> Self {
        match store::ConfigModel::global(cx).read(cx).config().ui_style {
            config::UiStyle::Vaxry => Skin::Vaxry,
            _ => Skin::Normal,
        }
    }
}

/// A `#rrggbb` from the palette, which is data rather than a theme role.
fn parse(color: &str) -> Option<Hsla> {
    gpui_kit::Rgba::try_from(color).ok().map(Into::into)
}

/// A rem measure in pixels, for the setters that take `Pixels` (`Cover::size`, `Icon::size`).
pub fn to_px(measure: Rems, window: &gpui_kit::Window) -> Pixels {
    measure.to_pixels(window.rem_size())
}

/// `text-5xl` — GPUI's own scale stops at `text_3xl`.
pub fn text_5xl() -> Rems {
    rems(3.)
}

/// `text-7xl`, the album detail's title.
pub fn text_7xl() -> Rems {
    rems(4.5)
}

/// `rounded-md` — 6px, the cover thumbs and tab pills.
pub fn radius_md() -> Rems {
    rems(0.375)
}

/// `text-[9px]`, the repeat-one badge.
pub fn text_9() -> Pixels {
    px(9.)
}

/// A borderless Button's colours in every state; a fill above 0.2 alpha, or a ring, needs [`dress`].
pub fn button(fill: Hsla, hover: Hsla, ink: Hsla, cx: &App) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .color(rest(fill))
        .hover(hover)
        .active(hover)
        .foreground(ink)
}

/// A button with no fill of its own that washes on hover, keeping `ink` throughout: what
/// `.ghost()` looks like when its foreground is not thrown away.
pub fn quiet_button(ink: Hsla, cx: &App) -> ButtonCustomVariant {
    button(gpui_kit::transparent_black(), white(HOVER, cx), ink, cx)
}

/// A button that paints nothing at all, for a frame around it that owns the wash.
pub fn bare_button(ink: Hsla, cx: &App) -> ButtonCustomVariant {
    let clear = gpui_kit::transparent_black();
    button(clear, clear, ink, cx)
}

/// gpui-component rests a custom variant's `color` at a fifth of its alpha; hover and active paint whole.
const VARIANT_REST: f32 = 0.2;

/// What to hand `.color()` so a borderless `fill` of at most 0.2 alpha rests as given; the border reads it whole.
pub fn rest(fill: Hsla) -> Hsla {
    let mut compensated = fill;
    compensated.a = (fill.a / VARIANT_REST).min(1.);
    compensated
}

/// `fill` rests on the instance at any alpha; the variant's `color` is the border in every state, so it takes `edge` whole.
pub fn dress(
    button: gpui_kit::component::button::Button,
    fill: Hsla,
    hover: Hsla,
    ink: Hsla,
    edge: Hsla,
    cx: &App,
) -> gpui_kit::component::button::Button {
    use gpui_kit::Styled as _;
    use gpui_kit::component::button::ButtonVariants as _;
    button
        .custom(
            ButtonCustomVariant::new(cx)
                .color(edge)
                .hover(hover)
                .active(hover)
                .foreground(ink),
        )
        .bg(fill)
        .border_color(edge)
}
