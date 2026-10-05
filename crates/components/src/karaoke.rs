//! Clip the sung layer against shaped text, preserving wrapping and syllable timing.

use std::ops::Range;

use gpui_kit::{
    App, Bounds, ContentMask, Element, ElementId, GlobalElementId, HighlightStyle, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, SharedString, StyledText, TextAlign, Window,
    WrappedLine, point, px, size,
};

pub struct KaraokeText {
    text: StyledText,
    source: SharedString,
    color: Hsla,
    bright: Vec<WrappedLine>,
    fills: Vec<(Range<usize>, f32)>,
}

impl KaraokeText {
    pub fn new(
        source: SharedString,
        color: Hsla,
        dim: Hsla,
        fills: Vec<(Range<usize>, f32)>,
    ) -> Self {
        let text = StyledText::new(source.clone()).with_highlights([(
            0..source.len(),
            HighlightStyle {
                color: Some(dim),
                ..Default::default()
            },
        )]);
        Self {
            text,
            source,
            color,
            bright: Vec::new(),
            fills,
        }
    }

    fn masks(&self, align: TextAlign) -> Vec<Bounds<Pixels>> {
        let layout = self.text.layout();
        let bounds = layout.bounds();
        let height = layout.line_height();
        let mut rows = Vec::new();
        let mut offset = 0;
        let mut y = bounds.origin.y;
        for line in layout.line_layouts() {
            let shaped = &line.unwrapped_layout;
            let ends = line
                .wrap_boundaries
                .iter()
                .map(|boundary| shaped.runs[boundary.run_ix].glyphs[boundary.glyph_ix].index)
                .chain([line.len()]);
            let mut start = 0;
            for end in ends {
                let left = shaped.x_for_index(start);
                let width = shaped.x_for_index(end) - left;
                let inset = match align {
                    TextAlign::Center => (bounds.size.width - width) / 2.,
                    TextAlign::Right => bounds.size.width - width,
                    _ => px(0.),
                };
                rows.push((
                    offset + start..offset + end,
                    shaped.clone(),
                    left,
                    bounds.origin.x + inset,
                    y,
                    offset,
                ));
                start = end;
                y += height;
            }
            offset += line.len() + 1;
        }
        let mut masks = Vec::new();
        for (range, fill) in &self.fills {
            if *fill <= 0. {
                continue;
            }
            let mut segments = Vec::new();
            for (row, shaped, left, x, y, offset) in &rows {
                let start = range.start.max(row.start);
                let end = range.end.min(row.end);
                if start >= end {
                    continue;
                }
                let from = shaped.x_for_index(start - offset);
                let to = shaped.x_for_index(end - offset);
                if to > from {
                    segments.push(Bounds::new(
                        point(*x + from - *left, *y),
                        size(to - from, height),
                    ));
                }
            }
            masks.extend(filled_segments(&segments, *fill));
        }
        masks
    }
}

fn filled_segments(segments: &[Bounds<Pixels>], fill: f32) -> Vec<Bounds<Pixels>> {
    let mut remaining = segments
        .iter()
        .map(|segment| segment.size.width)
        .sum::<Pixels>()
        * fill.clamp(0., 1.);
    segments
        .iter()
        .filter_map(|segment| {
            let width = remaining.min(segment.size.width);
            remaining -= width;
            (width > px(0.)).then(|| Bounds::new(segment.origin, size(width, segment.size.height)))
        })
        .collect()
}

impl IntoElement for KaraokeText {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for KaraokeText {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        self.text.request_layout(id, inspector, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.text.prepaint(id, inspector, bounds, state, window, cx);
        let layouts = self.text.layout().line_layouts();
        let Some(first) = layouts.first() else { return };
        let mut style = window.text_style();
        style.color = self.color;
        let run = style.to_run(self.source.len());
        self.bright = window
            .text_system()
            .shape_text(
                self.source.clone(),
                first.font_size(),
                &[run],
                first.wrap_width,
                None,
            )
            .map(|lines| lines.into_vec())
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "could not shape karaoke text");
                Vec::new()
            });
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut (),
        prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.text
            .paint(id, inspector, bounds, state, prepaint, window, cx);
        let layout = self.text.layout();
        let text_bounds = layout.bounds();
        let line_height = layout.line_height();
        let align = window.text_style().text_align;
        for mask in self.masks(align) {
            window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
                let mut origin = text_bounds.origin;
                for line in &self.bright {
                    if let Err(error) =
                        line.paint(origin, line_height, align, Some(text_bounds), window, cx)
                    {
                        tracing::warn!(%error, "could not paint karaoke text");
                    }
                    origin.y += line.size(line_height).height;
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wrapped_word_fills_across_rows_without_lighting_the_next_row_early() {
        let rows = [
            Bounds::new(point(px(10.), px(20.)), size(px(60.), px(24.))),
            Bounds::new(point(px(5.), px(44.)), size(px(40.), px(24.))),
        ];
        let halfway = filled_segments(&rows, 0.5);
        assert_eq!(halfway.len(), 1);
        assert_eq!(halfway[0].size.width, px(50.));
        let later = filled_segments(&rows, 0.8);
        assert_eq!(later[0], rows[0]);
        assert_eq!(later[1].origin, rows[1].origin);
        assert_eq!(later[1].size.width, px(20.));
        assert!(filled_segments(&rows, 0.).is_empty());
        assert_eq!(filled_segments(&rows, 1.), rows);
    }

    struct Fixture {
        text: gpui_kit::SharedString,
        fill: f32,
        layout: std::rc::Rc<std::cell::RefCell<Option<gpui_kit::TextLayout>>>,
    }

    impl gpui_kit::Render for Fixture {
        fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
            use gpui_kit::{ParentElement as _, Styled as _};
            let karaoke = KaraokeText::new(
                self.text.clone(),
                gpui_kit::white(),
                gpui_kit::black(),
                vec![(0..self.text.len(), self.fill)],
            );
            *self.layout.borrow_mut() = Some(karaoke.text.layout().clone());
            gpui_kit::div()
                .w(px(160.))
                .text_size(px(24.))
                .text_center()
                .child(karaoke)
        }
    }

    #[gpui_kit::test]
    fn native_text_keeps_identical_wrapping_as_words_fill(cx: &mut gpui_kit::TestAppContext) {
        use gpui_kit::AppContext as _;
        let layout = std::rc::Rc::new(std::cell::RefCell::new(None));
        let view = cx.add_window({
            let layout = layout.clone();
            move |_, _| Fixture {
                text: "深海 52Hz — words across several wrapped lines".into(),
                fill: 0.,
                layout,
            }
        });
        let before = cx
            .update_window(view.into(), |_, window, cx| {
                window.draw(cx).clear(cx);
                let layout = layout.borrow();
                let layout = layout.as_ref().unwrap();
                (layout.bounds(), layout.wrapped_text())
            })
            .unwrap();
        cx.update_window(view.into(), |root, window, cx| {
            root.downcast::<Fixture>().unwrap().update(cx, |view, cx| {
                view.fill = 0.5;
                cx.notify();
            });
            window.draw(cx).clear(cx);
        })
        .unwrap();
        let layout = layout.borrow();
        let layout = layout.as_ref().unwrap();
        assert_eq!(layout.bounds(), before.0);
        assert_eq!(layout.wrapped_text(), before.1);
        assert!(layout.bounds().size.height > layout.line_height());
    }
}
