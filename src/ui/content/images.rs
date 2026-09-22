use super::*;
use crate::image_service::{MathMetrics, MathRenderStyle, MATH_PADDING_EMS};

const INLINE_MATH_REFERENCE_EMS: f32 = 1.25;
const DISPLAY_MATH_REFERENCE_EMS: f32 = 2.5;
const MATH_CELL_TOLERANCE: f32 = 0.15;
const MATH_INK_OVERSHOOT_EMS: f32 = 0.03;
const TEXT_BASELINE_IN_ROW: f32 = 0.78;
const MATH_BASELINE_SLACK: f32 = 0.1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct MathRaster {
    pub(super) pixel_height: u32,
    pub(super) offset_y: i32,
}

#[derive(Clone)]
pub(super) enum MathBlockRenderState {
    Ready { image_key: String, size: Size, raster: MathRaster },
    Pending { height: u16 },
    Failed { height: u16 },
    Unsupported { height: u16 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum InlineMathRenderState {
    Ready { image_key: String, size: Size, text_row: u16, raster: MathRaster },
    Pending,
    Failed,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InlineMathPlacement {
    pub(super) expression_index: usize,
    pub(super) rect: Rect,
}

impl MathBlockRenderState {
    pub(super) fn height(&self) -> u16 {
        match self {
            Self::Ready { size, .. } => size.height.saturating_add(2),
            Self::Pending { height } | Self::Failed { height } | Self::Unsupported { height } => *height,
        }
    }
}

fn indexed_color_rgb(index: u8) -> [u8; 3] {
    const ANSI: [[u8; 3]; 16] = [[0, 0, 0], [205, 49, 49], [13, 188, 121], [229, 229, 16], [36, 114, 200], [188, 63, 188], [17, 168, 205], [229, 229, 229], [102, 102, 102], [241, 76, 76], [35, 209, 139], [245, 245, 67], [59, 142, 234], [214, 112, 214], [41, 184, 219], [255, 255, 255]];
    match index {
        0..=15 => ANSI[index as usize],
        16..=231 => {
            let cube = index - 16;
            let component = |value: u8| if value == 0 { 0 } else { 55 + value * 40 };
            [component(cube / 36), component((cube % 36) / 6), component(cube % 6)]
        }
        _ => {
            let value = 8 + (index - 232) * 10;
            [value, value, value]
        }
    }
}

fn terminal_color_rgb(color: ratatui::style::Color, fallback: ratatui::style::Color) -> [u8; 3] {
    use ratatui::style::Color;
    match color {
        Color::Reset => terminal_color_rgb(fallback, Color::Gray),
        Color::Black => indexed_color_rgb(0),
        Color::Red => indexed_color_rgb(1),
        Color::Green => indexed_color_rgb(2),
        Color::Yellow => indexed_color_rgb(3),
        Color::Blue => indexed_color_rgb(4),
        Color::Magenta => indexed_color_rgb(5),
        Color::Cyan => indexed_color_rgb(6),
        Color::Gray => indexed_color_rgb(7),
        Color::DarkGray => indexed_color_rgb(8),
        Color::LightRed => indexed_color_rgb(9),
        Color::LightGreen => indexed_color_rgb(10),
        Color::LightYellow => indexed_color_rgb(11),
        Color::LightBlue => indexed_color_rgb(12),
        Color::LightMagenta => indexed_color_rgb(13),
        Color::LightCyan => indexed_color_rgb(14),
        Color::White => indexed_color_rgb(15),
        Color::Rgb(red, green, blue) => [red, green, blue],
        Color::Indexed(index) => indexed_color_rgb(index),
    }
}

fn math_image_key(latex: &str, color: [u8; 3], style: MathRenderStyle) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "ratex-0.1.14-v3".hash(&mut hasher);
    latex.hash(&mut hasher);
    color.hash(&mut hasher);
    style.hash(&mut hasher);
    format!("math:{:016x}", hasher.finish())
}

fn display_math_state_key(item_index: usize, image_key: &str) -> String {
    format!("math:block:{item_index}:{image_key}")
}

fn inline_math_state_prefix(item_index: usize, expression_index: usize) -> String {
    format!("math:inline:{item_index}:{expression_index}:")
}

fn inline_math_state_key(item_index: usize, expression_index: usize, text_row: u16, image_key: &str) -> String {
    format!("{}{text_row}:{image_key}", inline_math_state_prefix(item_index, expression_index))
}

pub(super) fn cached_inline_math_state(app: &App, item_index: usize, expression_index: usize) -> InlineMathRenderState {
    let prefix = inline_math_state_prefix(item_index, expression_index);
    app.images
        .image_states
        .iter()
        .find_map(|(key, state)| {
            let (text_row, image_key) = key.strip_prefix(&prefix)?.split_once(':')?;
            Some(InlineMathRenderState::Ready { image_key: image_key.to_string(), size: state.size, text_row: text_row.parse().ok()?, raster: MathRaster::default() })
        })
        .unwrap_or(InlineMathRenderState::Unsupported)
}

fn math_cells(extent: f32, limit: u16) -> u16 {
    ((extent - MATH_CELL_TOLERANCE).ceil().max(1.0) as u16).min(limit.max(1))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct MathGeometry {
    pub(super) size: Size,
    rows_per_em: f32,
    pub(super) content_rows: f32,
    ascent_rows: f32,
}

pub(super) fn math_geometry(metrics: MathMetrics, font_size: ratatui_image::FontSize, rows_per_em: f32, limits: Size) -> MathGeometry {
    let cell_aspect = f32::from(font_size.height.max(1)) / f32::from(font_size.width.max(1));
    let columns_per_row = (metrics.width + 2.0 * MATH_PADDING_EMS) * cell_aspect;
    let content_ems = (metrics.height + metrics.depth).max(0.0) + 2.0 * MATH_INK_OVERSHOOT_EMS;
    let fit = |width: f32, height: f32, rows_per_em: f32| (width / (columns_per_row * rows_per_em)).min(height / (content_ems * rows_per_em)).min(1.0);
    let rows_per_em = rows_per_em * fit(f32::from(limits.width.max(1)), f32::from(limits.height.max(1)), rows_per_em);
    let size = Size::new(math_cells(columns_per_row * rows_per_em, limits.width), math_cells(content_ems * rows_per_em, limits.height));
    let rows_per_em = rows_per_em * fit(f32::from(size.width), f32::from(size.height), rows_per_em);
    MathGeometry { size, rows_per_em, content_rows: content_ems * rows_per_em, ascent_rows: (metrics.height.max(0.0) + MATH_INK_OVERSHOOT_EMS) * rows_per_em }
}

impl MathGeometry {
    pub(super) fn inline_text_row(&self) -> u16 {
        ((self.ascent_rows - MATH_BASELINE_SLACK).floor().max(0.0) as u16).min(self.size.height.saturating_sub(1))
    }

    fn raster(&self, font_size: ratatui_image::FontSize, content_top: f32) -> MathRaster {
        let row_pixels = f32::from(font_size.height.max(1));
        let padding_rows = (MATH_PADDING_EMS - MATH_INK_OVERSHOOT_EMS) * self.rows_per_em;
        MathRaster { pixel_height: ((self.content_rows + 2.0 * padding_rows) * row_pixels).round().max(1.0) as u32, offset_y: ((content_top - padding_rows) * row_pixels).round() as i32 }
    }

    pub(super) fn inline_raster(&self, font_size: ratatui_image::FontSize) -> MathRaster {
        let slack = (f32::from(self.size.height) - self.content_rows).max(0.0);
        let content_top = (f32::from(self.inline_text_row()) + TEXT_BASELINE_IN_ROW - self.ascent_rows).clamp(0.0, slack);
        self.raster(font_size, content_top)
    }

    pub(super) fn centered_raster(&self, font_size: ratatui_image::FontSize) -> MathRaster {
        self.raster(font_size, ((f32::from(self.size.height) - self.content_rows) / 2.0).max(0.0))
    }
}

pub(super) fn math_block_marker(marker: &str) -> &str {
    match marker {
        "-" | "*" | "+" => "•",
        _ => marker,
    }
}

pub(super) fn math_block_offset(marker: &str, indent: u16) -> u16 {
    let marker = math_block_marker(marker);
    if marker.is_empty() {
        indent
    } else {
        indent.saturating_add(u16::try_from(marker.width() + 1).unwrap_or(u16::MAX))
    }
}

fn failed_math_height(latex: &str, width: u16, block_height: u16) -> u16 {
    let source_width = latex.split_whitespace().map(|word| word.width() + 1).sum::<usize>();
    let source_rows = source_width.div_ceil(usize::from(width.max(1))).max(1);
    u16::try_from(source_rows + 1).unwrap_or(u16::MAX).clamp(2, block_height.max(2))
}

struct DocumentMath {
    blocks: Vec<(usize, String, u16)>,
    inline: Vec<(usize, Vec<(String, bool)>)>,
}

fn collect_document_math(app: &App) -> DocumentMath {
    let Some(document) = app.document() else {
        return DocumentMath { blocks: Vec::new(), inline: Vec::new() };
    };
    let mut math = DocumentMath { blocks: Vec::new(), inline: Vec::new() };
    for (index, item) in app.document.content_items.iter().enumerate() {
        match item {
            ContentItem::MathBlock { range, marker, indent, .. } => math.blocks.push((index, document.slice(*range).trim().to_string(), math_block_offset(document.slice(*marker), *indent))),
            ContentItem::TextLine { range, heading_level: 0, .. } | ContentItem::TaskItem { text: range, .. } => {
                let expressions: Vec<(String, bool)> = crate::core::markdown::inline_math(document.slice(*range)).into_iter().map(|expression| (expression.source.to_string(), expression.display)).collect();
                if !expressions.is_empty() {
                    math.inline.push((index, expressions));
                }
            }
            _ => {}
        }
    }
    math
}

fn document_macro_preamble(math: &DocumentMath) -> String {
    let mut seen = std::collections::HashSet::new();
    math.blocks.iter().map(|(_, latex, _)| latex.as_str()).chain(math.inline.iter().flat_map(|(_, expressions)| expressions.iter().map(|(latex, _)| latex.as_str()))).flat_map(crate::core::latex::macro_definitions).filter(|definition| seen.insert(*definition)).collect()
}

fn with_preamble(preamble: &str, latex: String) -> String {
    if preamble.is_empty() {
        latex
    } else {
        format!("{preamble}{latex}")
    }
}

pub(super) fn prepare_math(app: &mut App, viewport: Size, render_images: bool) -> (Vec<Option<MathBlockRenderState>>, Vec<Vec<InlineMathRenderState>>) {
    let math = collect_document_math(app);
    let preamble = document_macro_preamble(&math);
    let mut blocks = vec![None; app.document.content_items.len()];
    let mut inline = vec![Vec::new(); app.document.content_items.len()];
    let fallback_color = app.state.theme.foreground;
    let color = terminal_color_rgb(app.state.theme.content.text, fallback_color);
    let font_size = render_images.then(|| app.images.picker.as_ref().map(|picker| picker.font_size())).flatten();
    let block_height = app.state.config.effective_latex_height();
    let display_rows_per_em = f32::from(block_height.saturating_sub(2).max(1)) / DISPLAY_MATH_REFERENCE_EMS;
    for (item_index, latex, offset) in math.blocks {
        let Some(font_size) = font_size else {
            blocks[item_index] = Some(MathBlockRenderState::Unsupported { height: block_height });
            continue;
        };
        let available_width = viewport.width.saturating_sub(6).saturating_sub(offset).max(1);
        let image_key = math_image_key(&with_preamble(&preamble, latex.clone()), color, MathRenderStyle::Display);
        blocks[item_index] = Some(if app.image_load_failed(&image_key) {
            MathBlockRenderState::Failed { height: failed_math_height(&latex, available_width, block_height) }
        } else if let Some(metrics) = app.math_metrics(&image_key) {
            let geometry = math_geometry(metrics, font_size, display_rows_per_em, Size::new(available_width, viewport.height.saturating_sub(2).max(1)));
            MathBlockRenderState::Ready { image_key, size: geometry.size, raster: geometry.centered_raster(font_size) }
        } else {
            if !app.is_image_pending(&image_key) {
                app.request_math_image(&image_key, with_preamble(&preamble, latex), color, MathRenderStyle::Display);
            }
            MathBlockRenderState::Pending { height: block_height }
        });
    }
    let inline_rows_per_em = f32::from(app.state.config.effective_inline_latex_height()) / INLINE_MATH_REFERENCE_EMS;
    let inline_limits = Size::new(viewport.width.saturating_sub(6).max(1), viewport.height.max(1));
    for (item_index, expressions) in math.inline {
        for (latex, display) in expressions {
            let Some(font_size) = font_size else {
                inline[item_index].push(InlineMathRenderState::Unsupported);
                continue;
            };
            let style = if display { MathRenderStyle::Display } else { MathRenderStyle::Inline };
            let latex = with_preamble(&preamble, latex);
            let image_key = math_image_key(&latex, color, style);
            inline[item_index].push(if app.image_load_failed(&image_key) {
                InlineMathRenderState::Failed
            } else if let Some(metrics) = app.math_metrics(&image_key) {
                let geometry = math_geometry(metrics, font_size, inline_rows_per_em, inline_limits);
                InlineMathRenderState::Ready { image_key, size: geometry.size, text_row: geometry.inline_text_row(), raster: geometry.inline_raster(font_size) }
            } else {
                if !app.is_image_pending(&image_key) {
                    app.request_math_image(&image_key, latex, color, style);
                }
                InlineMathRenderState::Pending
            });
        }
    }
    (blocks, inline)
}

fn rasterize_math(image: &image::DynamicImage, raster: MathRaster, size: Size, font_size: ratatui_image::FontSize) -> image::DynamicImage {
    let height = raster.pixel_height.max(1);
    let width = ((f64::from(image.width()) * f64::from(height) / f64::from(image.height().max(1))).round() as u32).max(1);
    let scaled = image.resize_exact(width, height, image::imageops::FilterType::Triangle).to_rgba8();
    let mut canvas = image::RgbaImage::new(u32::from(size.width) * u32::from(font_size.width), u32::from(size.height) * u32::from(font_size.height));
    image::imageops::overlay(&mut canvas, &scaled, 0, i64::from(raster.offset_y));
    image::DynamicImage::ImageRgba8(canvas)
}

fn ensure_math_image_state(app: &mut App, state_key: String, image_key: &str, size: Size, raster: MathRaster) -> String {
    if app.touch_image_state(&state_key, size) || size.width == 0 || size.height == 0 {
        return state_key;
    }
    app.remove_image_state(&state_key);
    let Some(image) = app.decoded_image(image_key) else {
        app.reload_image(image_key);
        return state_key;
    };
    let Some(picker) = app.images.picker.as_ref() else {
        return state_key;
    };
    let font_size = picker.font_size();
    let protocol_bytes = usize::from(size.width) * usize::from(font_size.width) * usize::from(size.height) * usize::from(font_size.height) * 4;
    if let Ok(protocol) = SlicedProtocol::new_with_resize(picker, rasterize_math(image.as_ref(), raster, size, font_size), size, Resize::Fit(None)) {
        app.insert_image_state(state_key.clone(), protocol, size, protocol_bytes);
    }
    state_key
}

pub(super) struct MathBlockView<'a> {
    pub(super) item_index: usize,
    pub(super) latex: &'a str,
    pub(super) state: &'a MathBlockRenderState,
    pub(super) viewport: Rect,
    pub(super) is_cursor: bool,
    pub(super) marker: &'a str,
    pub(super) indent: u16,
}

pub(super) fn render_math_block(f: &mut Frame, app: &mut App, view: MathBlockView<'_>, area: Rect) {
    if view.is_cursor {
        f.render_widget(Paragraph::new("").style(Style::default().bg(app.state.theme.selection)), area);
    }
    let marker = math_block_marker(view.marker);
    let offset = math_block_offset(view.marker, view.indent);
    let body_x = area.x.saturating_add(2).saturating_add(offset);
    let body_width = area.width.saturating_sub(2).saturating_sub(offset);
    if !marker.is_empty() {
        let marker_area = Rect { x: area.x.saturating_add(2).saturating_add(view.indent), width: body_x.saturating_sub(area.x.saturating_add(2).saturating_add(view.indent)), height: 1.min(area.height), ..area };
        f.render_widget(Paragraph::new(Span::styled(marker, Style::default().fg(app.state.theme.content.list_marker))), marker_area);
    }
    let content_area = Rect { x: body_x, width: body_width, ..area };
    match view.state {
        MathBlockRenderState::Ready { image_key, size, raster } => {
            let image_area = Rect { x: body_x.saturating_add(body_width.saturating_sub(size.width) / 2), y: area.y.saturating_add(1), width: size.width.min(body_width), height: size.height.min(area.height.saturating_sub(1)) }.intersection(view.viewport);
            if image_area.width > 0 && image_area.height > 0 {
                let state_key = ensure_math_image_state(app, display_math_state_key(view.item_index, image_key), image_key, *size, *raster);
                if let Some(image_state) = app.images.image_states.get(&state_key) {
                    f.render_widget(SlicedImage::new(&image_state.image, SignedPosition::from((0, 0))), image_area);
                }
            }
        }
        MathBlockRenderState::Pending { .. } => {
            let source = view.latex.split_whitespace().collect::<Vec<_>>().join(" ");
            let text = vec![Line::from(Span::styled("∑ Rendering equation…", Style::default().fg(app.state.theme.secondary).add_modifier(Modifier::ITALIC))), Line::from(Span::styled(source, Style::default().fg(app.state.theme.muted)))];
            f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), content_area);
        }
        MathBlockRenderState::Failed { .. } => {
            let source = view.latex.split_whitespace().collect::<Vec<_>>().join(" ");
            let text = vec![Line::from(Span::styled("⚠ Equation could not be rendered", Style::default().fg(app.state.theme.error).add_modifier(Modifier::BOLD))), Line::from(Span::styled(source, Style::default().fg(app.state.theme.muted)))];
            f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), content_area);
        }
        MathBlockRenderState::Unsupported { .. } => {
            let source = view.latex.split_whitespace().collect::<Vec<_>>().join(" ");
            f.render_widget(Paragraph::new(Line::from(vec![Span::styled("∑ ", Style::default().fg(app.state.theme.secondary)), Span::styled(source, Style::default().fg(app.state.theme.content.code))])).wrap(Wrap { trim: true }), content_area);
        }
    }
    let indicator = if view.is_cursor { "▶" } else { " " };
    f.render_widget(Paragraph::new(Span::styled(indicator, Style::default().fg(app.state.theme.warning))), Rect { width: 1, ..area });
}

pub(super) fn render_inline_math(f: &mut Frame, app: &mut App, item_index: usize, states: &[InlineMathRenderState], placements: &[InlineMathPlacement], viewport: Rect) {
    for placement in placements {
        let Some(InlineMathRenderState::Ready { image_key, size, text_row, raster }) = states.get(placement.expression_index) else {
            continue;
        };
        let area = placement.rect.intersection(viewport);
        if area.width == 0 || area.height == 0 {
            continue;
        }
        let state_key = ensure_math_image_state(app, inline_math_state_key(item_index, placement.expression_index, *text_row, image_key), image_key, *size, *raster);
        if let Some(image_state) = app.images.image_states.get(&state_key) {
            f.render_widget(SlicedImage::new(&image_state.image, SignedPosition::from((0, 0))), area);
        }
    }
}

pub(super) fn standalone_image_state_key(item_index: usize, resolved_path: &str) -> String {
    format!("standalone:{item_index}:{resolved_path}")
}

pub(super) fn inline_image_state_key(item_index: usize, selection_index: usize, resolved_path: &str) -> String {
    format!("inline:{item_index}:{selection_index}:{resolved_path}")
}

pub(super) fn ensure_image_state(app: &mut App, state_key: &str, resolved_path: Option<&std::path::Path>, resolved_path_str: &str, normalized_path: &str, is_remote: bool, size: Size) {
    if app.touch_image_state(state_key, size) || size.width == 0 || size.height == 0 {
        return;
    }
    app.remove_image_state(state_key);
    let image = app.decoded_image(resolved_path_str);
    let Some(image) = image else {
        app.request_image_load(resolved_path_str, resolved_path, is_remote.then_some(normalized_path));
        return;
    };
    let Some(picker) = app.images.picker.as_ref() else {
        return;
    };
    let source_bytes = crate::image_service::decoded_image_bytes(image.as_ref());
    let Ok(protocol) = SlicedProtocol::new_with_resize(picker, image.as_ref().clone(), size, Resize::Fit(None)) else {
        return;
    };
    app.insert_image_state(state_key.to_string(), protocol, size, source_bytes);
}

pub(super) fn inline_thumbnail_width(area_width: u16, image_height: u16) -> u16 {
    let available_width = area_width.saturating_sub(INLINE_THUMBNAIL_HORIZONTAL_PADDING * 2);
    image_height.saturating_mul(2).clamp(INLINE_THUMBNAIL_MIN_WIDTH, INLINE_THUMBNAIL_MAX_WIDTH).min(available_width.max(1))
}

pub(super) fn inline_thumbnails_per_row(area_width: u16, image_height: u16) -> usize {
    let available_width = area_width.saturating_sub(INLINE_THUMBNAIL_HORIZONTAL_PADDING * 2);
    let thumbnail_width = inline_thumbnail_width(area_width, image_height);
    usize::from(available_width.saturating_add(INLINE_THUMBNAIL_GAP) / thumbnail_width.saturating_add(INLINE_THUMBNAIL_GAP).max(1)).max(1)
}

pub(super) fn inline_thumbnails_height(image_count: usize, area_width: u16, image_height: u16) -> u16 {
    if image_count == 0 {
        return 0;
    }
    let per_row = inline_thumbnails_per_row(area_width, image_height);
    let rows = image_count.saturating_add(per_row - 1) / per_row;
    u16::try_from(rows).unwrap_or(u16::MAX).saturating_mul(image_height)
}

pub(super) fn image_frame_borders(visible_height: u16, configured_height: u16) -> Borders {
    let mut borders = Borders::LEFT | Borders::RIGHT;
    if visible_height > 1 {
        borders |= Borders::TOP;
    }
    if visible_height >= configured_height {
        borders |= Borders::BOTTOM;
    }
    borders
}

pub(super) fn visible_item_height(total_height: u16, viewport_height: u16, item_height: u16) -> u16 {
    viewport_height.saturating_sub(total_height).min(item_height)
}

#[cfg(test)]
pub(super) fn inline_prose_text(text: &str, theme: &Theme) -> String {
    inline_prose_text_with_math(text, theme, &[])
}

pub(super) fn inline_prose_text_with_math(text: &str, theme: &Theme, math_states: &[InlineMathRenderState]) -> String {
    parse_inline_formatting_with_math::<fn(&str) -> bool>(text, theme, None, None, math_states).iter().map(|span| span.content.as_ref()).collect::<String>().trim().to_string()
}
