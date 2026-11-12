// todo("windows"): remove
#![cfg_attr(windows, allow(dead_code))]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    AtlasTextureId, AtlasTile, Background, Bounds, ContentMask, Corners, Edges, Hsla, Pixels,
    Point, Radians, ScaledPixels, Size, bounds_tree::BoundsTree, point,
};
use std::{
    fmt::Debug,
    iter::Peekable,
    ops::{Add, Range, Sub},
    slice,
};

#[allow(non_camel_case_types, unused)]
pub(crate) type PathVertex_ScaledPixels = PathVertex<ScaledPixels>;

pub(crate) type DrawOrder = u32;

#[derive(Default)]
pub(crate) struct Scene {
    pub(crate) paint_operations: Vec<PaintOperation>,
    primitive_bounds: BoundsTree<ScaledPixels>,
    layer_stack: Vec<DrawOrder>,
    max_order: DrawOrder,
    order_floor: DrawOrder,
    pub(crate) shadows: Vec<Shadow>,
    pub(crate) quads: Vec<Quad>,
    pub(crate) paths: Vec<Path<ScaledPixels>>,
    pub(crate) underlines: Vec<Underline>,
    pub(crate) monochrome_sprites: Vec<MonochromeSprite>,
    pub(crate) polychrome_sprites: Vec<PolychromeSprite>,
    pub(crate) surfaces: Vec<PaintSurface>,
}

impl Scene {
    pub fn clear(&mut self) {
        self.paint_operations.clear();
        self.primitive_bounds.clear();
        self.layer_stack.clear();
        self.max_order = 0;
        self.order_floor = 0;
        self.paths.clear();
        self.shadows.clear();
        self.quads.clear();
        self.underlines.clear();
        self.monochrome_sprites.clear();
        self.polychrome_sprites.clear();
        self.surfaces.clear();
    }

    pub fn len(&self) -> usize {
        self.paint_operations.len()
    }

    pub fn push_layer(&mut self, bounds: Bounds<ScaledPixels>) {
        let order = if self.order_floor == 0 {
            self.primitive_bounds.insert(bounds)
        } else {
            // Post-backdrop draws in an active layer aren't in the bounds
            // tree and may even lie outside its reservation. A new layer must
            // therefore start after all of them, not merely at the floor.
            self.primitive_bounds
                .insert_with_min_order(bounds, self.max_order + 1)
        };
        self.max_order = self.max_order.max(order);
        self.layer_stack.push(order);
        self.paint_operations
            .push(PaintOperation::StartLayer(bounds));
    }

    pub fn pop_layer(&mut self) {
        self.layer_stack.pop();
        if self.order_floor != 0 {
            // A layer active across a backdrop may have a reservation older
            // than its subsequent draws. Do not let later draws pass those
            // draws after leaving it, including when returning to a parent.
            self.order_floor = self.max_order + 1;
        }
        self.paint_operations.push(PaintOperation::EndLayer);
    }

    pub fn insert_primitive(&mut self, primitive: impl Into<Primitive>) {
        let mut primitive = primitive.into();
        let clipped_bounds = primitive
            .bounds()
            .intersect(&primitive.content_mask().bounds);

        if clipped_bounds.is_empty() {
            return;
        }

        let blur_sigma = match &primitive {
            Primitive::Quad(quad) => backdrop_blur_sigma(quad),
            _ => None,
        };
        let order = if let Some(sigma) = blur_sigma {
            // Layer reservations do not necessarily cover their primitives or a
            // backdrop's sample region. A global barrier also accounts for layers
            // already closed and avoids retroactively inserting all their glyphs.
            let order = self.primitive_bounds.insert_with_min_order(
                backdrop_blur_sample_bounds(clipped_bounds, sigma),
                self.max_order + 1,
            );
            self.order_floor = order + 1;
            order
        } else if let Some(layer_order) = self.layer_stack.last() {
            (*layer_order).max(self.order_floor)
        } else {
            self.primitive_bounds
                .insert_with_min_order(clipped_bounds, self.order_floor.max(1))
        };
        self.max_order = self.max_order.max(order);
        match &mut primitive {
            Primitive::Shadow(shadow) => {
                shadow.order = order;
                self.shadows.push(shadow.clone());
            }
            Primitive::Quad(quad) => {
                quad.order = order;
                self.quads.push(quad.clone());
            }
            Primitive::Path(path) => {
                path.order = order;
                path.id = PathId(self.paths.len());
                self.paths.push(path.clone());
            }
            Primitive::Underline(underline) => {
                underline.order = order;
                self.underlines.push(underline.clone());
            }
            Primitive::MonochromeSprite(sprite) => {
                sprite.order = order;
                self.monochrome_sprites.push(sprite.clone());
            }
            Primitive::PolychromeSprite(sprite) => {
                sprite.order = order;
                self.polychrome_sprites.push(sprite.clone());
            }
            Primitive::Surface(surface) => {
                surface.order = order;
                self.surfaces.push(surface.clone());
            }
        }
        self.paint_operations
            .push(PaintOperation::Primitive(primitive));
    }

    pub fn replay(&mut self, range: Range<usize>, prev_scene: &Scene) {
        for operation in &prev_scene.paint_operations[range] {
            match operation {
                PaintOperation::Primitive(primitive) => self.insert_primitive(primitive.clone()),
                PaintOperation::StartLayer(bounds) => self.push_layer(*bounds),
                PaintOperation::EndLayer => self.pop_layer(),
            }
        }
    }

    pub fn finish(&mut self) {
        self.shadows.sort_by_key(|shadow| shadow.order);
        self.quads.sort_by_key(|quad| quad.order);
        self.paths.sort_by_key(|path| path.order);
        self.underlines.sort_by_key(|underline| underline.order);
        self.monochrome_sprites
            .sort_by_key(|sprite| (sprite.order, sprite.tile.tile_id));
        self.polychrome_sprites
            .sort_by_key(|sprite| (sprite.order, sprite.tile.tile_id));
        self.surfaces.sort_by_key(|surface| surface.order);
    }

    #[cfg_attr(
        all(
            any(target_os = "linux", target_os = "freebsd"),
            not(any(feature = "x11", feature = "wayland"))
        ),
        allow(dead_code)
    )]
    pub(crate) fn batches(&self) -> impl Iterator<Item = PrimitiveBatch<'_>> {
        BatchIterator {
            shadows: &self.shadows,
            shadows_start: 0,
            shadows_iter: self.shadows.iter().peekable(),
            quads: &self.quads,
            quads_start: 0,
            quads_iter: self.quads.iter().peekable(),
            paths: &self.paths,
            paths_start: 0,
            paths_iter: self.paths.iter().peekable(),
            underlines: &self.underlines,
            underlines_start: 0,
            underlines_iter: self.underlines.iter().peekable(),
            monochrome_sprites: &self.monochrome_sprites,
            monochrome_sprites_start: 0,
            monochrome_sprites_iter: self.monochrome_sprites.iter().peekable(),
            polychrome_sprites: &self.polychrome_sprites,
            polychrome_sprites_start: 0,
            polychrome_sprites_iter: self.polychrome_sprites.iter().peekable(),
            surfaces: &self.surfaces,
            surfaces_start: 0,
            surfaces_iter: self.surfaces.iter().peekable(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Default)]
#[cfg_attr(
    all(
        any(target_os = "linux", target_os = "freebsd"),
        not(any(feature = "x11", feature = "wayland"))
    ),
    allow(dead_code)
)]
pub(crate) enum PrimitiveKind {
    Shadow,
    #[default]
    Quad,
    Path,
    Underline,
    MonochromeSprite,
    PolychromeSprite,
    Surface,
}

pub(crate) enum PaintOperation {
    Primitive(Primitive),
    StartLayer(Bounds<ScaledPixels>),
    EndLayer,
}

#[derive(Clone)]
pub(crate) enum Primitive {
    Shadow(Shadow),
    Quad(Quad),
    Path(Path<ScaledPixels>),
    Underline(Underline),
    MonochromeSprite(MonochromeSprite),
    PolychromeSprite(PolychromeSprite),
    Surface(PaintSurface),
}

impl Primitive {
    pub fn bounds(&self) -> &Bounds<ScaledPixels> {
        match self {
            Primitive::Shadow(shadow) => &shadow.bounds,
            Primitive::Quad(quad) => &quad.bounds,
            Primitive::Path(path) => &path.bounds,
            Primitive::Underline(underline) => &underline.bounds,
            Primitive::MonochromeSprite(sprite) => &sprite.bounds,
            Primitive::PolychromeSprite(sprite) => &sprite.bounds,
            Primitive::Surface(surface) => &surface.bounds,
        }
    }

    pub fn content_mask(&self) -> &ContentMask<ScaledPixels> {
        match self {
            Primitive::Shadow(shadow) => &shadow.content_mask,
            Primitive::Quad(quad) => &quad.content_mask,
            Primitive::Path(path) => &path.content_mask,
            Primitive::Underline(underline) => &underline.content_mask,
            Primitive::MonochromeSprite(sprite) => &sprite.content_mask,
            Primitive::PolychromeSprite(sprite) => &sprite.content_mask,
            Primitive::Surface(surface) => &surface.content_mask,
        }
    }
}

#[cfg_attr(
    all(
        any(target_os = "linux", target_os = "freebsd"),
        not(any(feature = "x11", feature = "wayland"))
    ),
    allow(dead_code)
)]
struct BatchIterator<'a> {
    shadows: &'a [Shadow],
    shadows_start: usize,
    shadows_iter: Peekable<slice::Iter<'a, Shadow>>,
    quads: &'a [Quad],
    quads_start: usize,
    quads_iter: Peekable<slice::Iter<'a, Quad>>,
    paths: &'a [Path<ScaledPixels>],
    paths_start: usize,
    paths_iter: Peekable<slice::Iter<'a, Path<ScaledPixels>>>,
    underlines: &'a [Underline],
    underlines_start: usize,
    underlines_iter: Peekable<slice::Iter<'a, Underline>>,
    monochrome_sprites: &'a [MonochromeSprite],
    monochrome_sprites_start: usize,
    monochrome_sprites_iter: Peekable<slice::Iter<'a, MonochromeSprite>>,
    polychrome_sprites: &'a [PolychromeSprite],
    polychrome_sprites_start: usize,
    polychrome_sprites_iter: Peekable<slice::Iter<'a, PolychromeSprite>>,
    surfaces: &'a [PaintSurface],
    surfaces_start: usize,
    surfaces_iter: Peekable<slice::Iter<'a, PaintSurface>>,
}

impl<'a> Iterator for BatchIterator<'a> {
    type Item = PrimitiveBatch<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut orders_and_kinds = [
            (
                self.shadows_iter.peek().map(|s| s.order),
                PrimitiveKind::Shadow,
            ),
            (self.quads_iter.peek().map(|q| q.order), PrimitiveKind::Quad),
            (self.paths_iter.peek().map(|q| q.order), PrimitiveKind::Path),
            (
                self.underlines_iter.peek().map(|u| u.order),
                PrimitiveKind::Underline,
            ),
            (
                self.monochrome_sprites_iter.peek().map(|s| s.order),
                PrimitiveKind::MonochromeSprite,
            ),
            (
                self.polychrome_sprites_iter.peek().map(|s| s.order),
                PrimitiveKind::PolychromeSprite,
            ),
            (
                self.surfaces_iter.peek().map(|s| s.order),
                PrimitiveKind::Surface,
            ),
        ];
        orders_and_kinds.sort_by_key(|(order, kind)| (order.unwrap_or(u32::MAX), *kind));

        let first = orders_and_kinds[0];
        let second = orders_and_kinds[1];
        let (batch_kind, max_order_and_kind) = if first.0.is_some() {
            (first.1, (second.0.unwrap_or(u32::MAX), second.1))
        } else {
            return None;
        };

        match batch_kind {
            PrimitiveKind::Shadow => {
                let shadows_start = self.shadows_start;
                let mut shadows_end = shadows_start + 1;
                self.shadows_iter.next();
                while self
                    .shadows_iter
                    .next_if(|shadow| (shadow.order, batch_kind) < max_order_and_kind)
                    .is_some()
                {
                    shadows_end += 1;
                }
                self.shadows_start = shadows_end;
                Some(PrimitiveBatch::Shadows(
                    &self.shadows[shadows_start..shadows_end],
                ))
            }
            PrimitiveKind::Quad => {
                let quads_start = self.quads_start;
                let mut quads_end = quads_start + 1;
                self.quads_iter.next();
                while self
                    .quads_iter
                    .next_if(|quad| (quad.order, batch_kind) < max_order_and_kind)
                    .is_some()
                {
                    quads_end += 1;
                }
                self.quads_start = quads_end;
                Some(PrimitiveBatch::Quads(&self.quads[quads_start..quads_end]))
            }
            PrimitiveKind::Path => {
                let paths_start = self.paths_start;
                let mut paths_end = paths_start + 1;
                self.paths_iter.next();
                while self
                    .paths_iter
                    .next_if(|path| (path.order, batch_kind) < max_order_and_kind)
                    .is_some()
                {
                    paths_end += 1;
                }
                self.paths_start = paths_end;
                Some(PrimitiveBatch::Paths(&self.paths[paths_start..paths_end]))
            }
            PrimitiveKind::Underline => {
                let underlines_start = self.underlines_start;
                let mut underlines_end = underlines_start + 1;
                self.underlines_iter.next();
                while self
                    .underlines_iter
                    .next_if(|underline| (underline.order, batch_kind) < max_order_and_kind)
                    .is_some()
                {
                    underlines_end += 1;
                }
                self.underlines_start = underlines_end;
                Some(PrimitiveBatch::Underlines(
                    &self.underlines[underlines_start..underlines_end],
                ))
            }
            PrimitiveKind::MonochromeSprite => {
                let texture_id = self.monochrome_sprites_iter.peek().unwrap().tile.texture_id;
                let sprites_start = self.monochrome_sprites_start;
                let mut sprites_end = sprites_start + 1;
                self.monochrome_sprites_iter.next();
                while self
                    .monochrome_sprites_iter
                    .next_if(|sprite| {
                        (sprite.order, batch_kind) < max_order_and_kind
                            && sprite.tile.texture_id == texture_id
                    })
                    .is_some()
                {
                    sprites_end += 1;
                }
                self.monochrome_sprites_start = sprites_end;
                Some(PrimitiveBatch::MonochromeSprites {
                    texture_id,
                    sprites: &self.monochrome_sprites[sprites_start..sprites_end],
                })
            }
            PrimitiveKind::PolychromeSprite => {
                let texture_id = self.polychrome_sprites_iter.peek().unwrap().tile.texture_id;
                let sprites_start = self.polychrome_sprites_start;
                let mut sprites_end = self.polychrome_sprites_start + 1;
                self.polychrome_sprites_iter.next();
                while self
                    .polychrome_sprites_iter
                    .next_if(|sprite| {
                        (sprite.order, batch_kind) < max_order_and_kind
                            && sprite.tile.texture_id == texture_id
                    })
                    .is_some()
                {
                    sprites_end += 1;
                }
                self.polychrome_sprites_start = sprites_end;
                Some(PrimitiveBatch::PolychromeSprites {
                    texture_id,
                    sprites: &self.polychrome_sprites[sprites_start..sprites_end],
                })
            }
            PrimitiveKind::Surface => {
                let surfaces_start = self.surfaces_start;
                let mut surfaces_end = surfaces_start + 1;
                self.surfaces_iter.next();
                while self
                    .surfaces_iter
                    .next_if(|surface| (surface.order, batch_kind) < max_order_and_kind)
                    .is_some()
                {
                    surfaces_end += 1;
                }
                self.surfaces_start = surfaces_end;
                Some(PrimitiveBatch::Surfaces(
                    &self.surfaces[surfaces_start..surfaces_end],
                ))
            }
        }
    }
}

#[derive(Debug)]
#[cfg_attr(
    all(
        any(target_os = "linux", target_os = "freebsd"),
        not(any(feature = "x11", feature = "wayland"))
    ),
    allow(dead_code)
)]
pub(crate) enum PrimitiveBatch<'a> {
    Shadows(&'a [Shadow]),
    Quads(&'a [Quad]),
    Paths(&'a [Path<ScaledPixels>]),
    Underlines(&'a [Underline]),
    MonochromeSprites {
        texture_id: AtlasTextureId,
        sprites: &'a [MonochromeSprite],
    },
    PolychromeSprites {
        texture_id: AtlasTextureId,
        sprites: &'a [PolychromeSprite],
    },
    Surfaces(&'a [PaintSurface]),
}

#[derive(Default, Debug, Clone)]
#[repr(C)]
pub(crate) struct Quad {
    pub order: DrawOrder,
    pub border_style: BorderStyle,
    pub bounds: Bounds<ScaledPixels>,
    pub content_mask: ContentMask<ScaledPixels>,
    pub background: Background,
    pub border_color: Hsla,
    pub corner_radii: Corners<ScaledPixels>,
    pub border_widths: Edges<ScaledPixels>,
}

impl From<Quad> for Primitive {
    fn from(quad: Quad) -> Self {
        Primitive::Quad(quad)
    }
}

// Orbit: frosted glass. A quad marked with this border colour (and no border
// width) asks the Metal renderer to blur what is already drawn behind it before
// painting, the way CSS `backdrop-filter: blur()` does. The marker rides in the
// border colour because that is the only per-quad channel that reaches the
// renderer without changing the scene layout or layout-affecting types.
const BACKDROP_MARKER_HUE: f32 = 0.707_106_77;
const BACKDROP_FADE_MARKER_HUE: f32 = 0.618_034;
const BACKDROP_MARKER_SATURATION: f32 = 0.577_215_7;
const BACKDROP_MAX_SIGMA: f32 = 4096.;

/// The border colour that marks a quad as frosted glass: before drawing it, the
/// renderer blurs the backdrop by `sigma` device pixels. Give the quad no border
/// width; its background then tints the frost.
pub fn backdrop_blur_marker(sigma: f32) -> Hsla {
    Hsla {
        h: BACKDROP_MARKER_HUE,
        s: BACKDROP_MARKER_SATURATION,
        l: (sigma / BACKDROP_MAX_SIGMA).clamp(0., 1.),
        a: 0.,
    }
}

/// Like [`backdrop_blur_marker`], but smoothly increases blur coverage from
/// clear at the quad's top to fully frosted at its bottom. The Gaussian sigma
/// stays constant; only the mix of sharp and blurred backdrop changes, using
/// smoothstep over the original quad bounds even when clipped. Its background
/// tint is still uniform. Give the quad no border width.
pub fn backdrop_blur_fade_marker(sigma: f32) -> Hsla {
    Hsla {
        h: BACKDROP_FADE_MARKER_HUE,
        ..backdrop_blur_marker(sigma)
    }
}

/// Whether this build's renderer honours [`backdrop_blur_marker`] and
/// [`backdrop_blur_fade_marker`]. Only the macOS Metal backend runs the backdrop
/// pass; every other backend paints the marked quad as it always would.
pub fn backdrop_blur_supported() -> bool {
    cfg!(all(target_os = "macos", not(feature = "macos-blade")))
}

/// Re-encode a marker from logical points to device pixels. `paint_quad` owns
/// the scale factor, so call sites pass points and never a window.
pub(crate) fn scale_backdrop_marker(color: Hsla, scale_factor: f32) -> Hsla {
    match backdrop_blur_sigma_device(&color) {
        Some(device) => Hsla {
            l: (device * scale_factor / BACKDROP_MAX_SIGMA).clamp(0., 1.),
            ..color
        },
        None => color,
    }
}

/// The blur `quad` asks for in device pixels, if it carries
/// [`backdrop_blur_marker`] or [`backdrop_blur_fade_marker`].
pub(crate) fn backdrop_blur_sigma(quad: &Quad) -> Option<f32> {
    backdrop_blur_sigma_device(&quad.border_color)
}

/// Whether a marked quad fades coverage from clear at the top to frost below.
#[cfg(any(test, all(target_os = "macos", not(feature = "macos-blade"))))]
pub(crate) fn backdrop_blur_fades(quad: &Quad) -> bool {
    is_backdrop_marker(&quad.border_color) && quad.border_color.h == BACKDROP_FADE_MARKER_HUE
}

/// The Gaussian snapshot includes its approximately three-sigma reach and a
/// two-pixel safety margin, rounded out to whole device pixels.
pub(crate) fn backdrop_blur_sample_bounds(
    bounds: Bounds<ScaledPixels>,
    sigma: f32,
) -> Bounds<ScaledPixels> {
    let bleed = (sigma * 3.).ceil() + 2.;
    let left = (bounds.origin.x.0 - bleed).floor();
    let top = (bounds.origin.y.0 - bleed).floor();
    let right = (bounds.origin.x.0 + bounds.size.width.0 + bleed).ceil();
    let bottom = (bounds.origin.y.0 + bounds.size.height.0 + bleed).ceil();
    Bounds {
        origin: point(ScaledPixels(left), ScaledPixels(top)),
        size: Size {
            width: ScaledPixels(right - left),
            height: ScaledPixels(bottom - top),
        },
    }
}

/// Whether `color` is either backdrop marker. A style carries the frost request
/// in its border colour, and `Style::paint` copies it onto the quad's border
/// field, which is where the renderer reads it back.
pub(crate) fn is_backdrop_marker(color: &Hsla) -> bool {
    (color.h == BACKDROP_MARKER_HUE || color.h == BACKDROP_FADE_MARKER_HUE)
        && color.s == BACKDROP_MARKER_SATURATION
        && color.a == 0.
        && color.l > 0.
}

fn backdrop_blur_sigma_device(color: &Hsla) -> Option<f32> {
    is_backdrop_marker(color).then_some(color.l * BACKDROP_MAX_SIGMA)
}

#[derive(Debug, Clone)]
#[repr(C)]
pub(crate) struct Underline {
    pub order: DrawOrder,
    pub pad: u32, // align to 8 bytes
    pub bounds: Bounds<ScaledPixels>,
    pub content_mask: ContentMask<ScaledPixels>,
    pub color: Hsla,
    pub thickness: ScaledPixels,
    pub wavy: u32,
}

impl From<Underline> for Primitive {
    fn from(underline: Underline) -> Self {
        Primitive::Underline(underline)
    }
}

#[derive(Debug, Clone)]
#[repr(C)]
pub(crate) struct Shadow {
    pub order: DrawOrder,
    pub blur_radius: ScaledPixels,
    pub bounds: Bounds<ScaledPixels>,
    pub corner_radii: Corners<ScaledPixels>,
    pub content_mask: ContentMask<ScaledPixels>,
    pub color: Hsla,
}

impl From<Shadow> for Primitive {
    fn from(shadow: Shadow) -> Self {
        Primitive::Shadow(shadow)
    }
}

/// The style of a border.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[repr(C)]
pub enum BorderStyle {
    /// A solid border.
    #[default]
    Solid = 0,
    /// A dashed border.
    Dashed = 1,
}

/// A data type representing a 2 dimensional transformation that can be applied to an element.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct TransformationMatrix {
    /// 2x2 matrix containing rotation and scale,
    /// stored row-major
    pub rotation_scale: [[f32; 2]; 2],
    /// translation vector
    pub translation: [f32; 2],
}

impl Eq for TransformationMatrix {}

impl TransformationMatrix {
    /// The unit matrix, has no effect.
    pub fn unit() -> Self {
        Self {
            rotation_scale: [[1.0, 0.0], [0.0, 1.0]],
            translation: [0.0, 0.0],
        }
    }

    /// Move the origin by a given point
    pub fn translate(mut self, point: Point<ScaledPixels>) -> Self {
        self.compose(Self {
            rotation_scale: [[1.0, 0.0], [0.0, 1.0]],
            translation: [point.x.0, point.y.0],
        })
    }

    /// Clockwise rotation in radians around the origin
    pub fn rotate(self, angle: Radians) -> Self {
        self.compose(Self {
            rotation_scale: [
                [angle.0.cos(), -angle.0.sin()],
                [angle.0.sin(), angle.0.cos()],
            ],
            translation: [0.0, 0.0],
        })
    }

    /// Scale around the origin
    pub fn scale(self, size: Size<f32>) -> Self {
        self.compose(Self {
            rotation_scale: [[size.width, 0.0], [0.0, size.height]],
            translation: [0.0, 0.0],
        })
    }

    /// Perform matrix multiplication with another transformation
    /// to produce a new transformation that is the result of
    /// applying both transformations: first, `other`, then `self`.
    #[inline]
    pub fn compose(self, other: TransformationMatrix) -> TransformationMatrix {
        if other == Self::unit() {
            return self;
        }
        // Perform matrix multiplication
        TransformationMatrix {
            rotation_scale: [
                [
                    self.rotation_scale[0][0] * other.rotation_scale[0][0]
                        + self.rotation_scale[0][1] * other.rotation_scale[1][0],
                    self.rotation_scale[0][0] * other.rotation_scale[0][1]
                        + self.rotation_scale[0][1] * other.rotation_scale[1][1],
                ],
                [
                    self.rotation_scale[1][0] * other.rotation_scale[0][0]
                        + self.rotation_scale[1][1] * other.rotation_scale[1][0],
                    self.rotation_scale[1][0] * other.rotation_scale[0][1]
                        + self.rotation_scale[1][1] * other.rotation_scale[1][1],
                ],
            ],
            translation: [
                self.translation[0]
                    + self.rotation_scale[0][0] * other.translation[0]
                    + self.rotation_scale[0][1] * other.translation[1],
                self.translation[1]
                    + self.rotation_scale[1][0] * other.translation[0]
                    + self.rotation_scale[1][1] * other.translation[1],
            ],
        }
    }

    /// Apply transformation to a point, mainly useful for debugging
    pub fn apply(&self, point: Point<Pixels>) -> Point<Pixels> {
        let input = [point.x.0, point.y.0];
        let mut output = self.translation;
        for (i, output_cell) in output.iter_mut().enumerate() {
            for (k, input_cell) in input.iter().enumerate() {
                *output_cell += self.rotation_scale[i][k] * *input_cell;
            }
        }
        Point::new(output[0].into(), output[1].into())
    }
}

impl Default for TransformationMatrix {
    fn default() -> Self {
        Self::unit()
    }
}

#[derive(Clone, Debug)]
#[repr(C)]
pub(crate) struct MonochromeSprite {
    pub order: DrawOrder,
    pub pad: u32, // align to 8 bytes
    pub bounds: Bounds<ScaledPixels>,
    pub content_mask: ContentMask<ScaledPixels>,
    pub color: Hsla,
    pub tile: AtlasTile,
    pub transformation: TransformationMatrix,
}

impl From<MonochromeSprite> for Primitive {
    fn from(sprite: MonochromeSprite) -> Self {
        Primitive::MonochromeSprite(sprite)
    }
}

#[derive(Clone, Debug)]
#[repr(C)]
pub(crate) struct PolychromeSprite {
    pub order: DrawOrder,
    pub pad: u32, // align to 8 bytes
    pub grayscale: bool,
    pub opacity: f32,
    pub bounds: Bounds<ScaledPixels>,
    pub content_mask: ContentMask<ScaledPixels>,
    pub corner_radii: Corners<ScaledPixels>,
    pub tile: AtlasTile,
}

impl From<PolychromeSprite> for Primitive {
    fn from(sprite: PolychromeSprite) -> Self {
        Primitive::PolychromeSprite(sprite)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PaintSurface {
    pub order: DrawOrder,
    pub bounds: Bounds<ScaledPixels>,
    pub content_mask: ContentMask<ScaledPixels>,
    #[cfg(target_os = "macos")]
    pub image_buffer: core_video::pixel_buffer::CVPixelBuffer,
}

impl From<PaintSurface> for Primitive {
    fn from(surface: PaintSurface) -> Self {
        Primitive::Surface(surface)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PathId(pub(crate) usize);

/// A line made up of a series of vertices and control points.
#[derive(Clone, Debug)]
pub struct Path<P: Clone + Debug + Default + PartialEq> {
    pub(crate) id: PathId,
    pub(crate) order: DrawOrder,
    pub(crate) bounds: Bounds<P>,
    pub(crate) content_mask: ContentMask<P>,
    pub(crate) vertices: Vec<PathVertex<P>>,
    pub(crate) color: Background,
    start: Point<P>,
    current: Point<P>,
    contour_count: usize,
}

impl Path<Pixels> {
    /// Create a new path with the given starting point.
    pub fn new(start: Point<Pixels>) -> Self {
        Self {
            id: PathId(0),
            order: DrawOrder::default(),
            vertices: Vec::new(),
            start,
            current: start,
            bounds: Bounds {
                origin: start,
                size: Default::default(),
            },
            content_mask: Default::default(),
            color: Default::default(),
            contour_count: 0,
        }
    }

    /// Scale this path by the given factor.
    pub fn scale(&self, factor: f32) -> Path<ScaledPixels> {
        Path {
            id: self.id,
            order: self.order,
            bounds: self.bounds.scale(factor),
            content_mask: self.content_mask.scale(factor),
            vertices: self
                .vertices
                .iter()
                .map(|vertex| vertex.scale(factor))
                .collect(),
            start: self.start.map(|start| start.scale(factor)),
            current: self.current.scale(factor),
            contour_count: self.contour_count,
            color: self.color,
        }
    }

    /// Move the start, current point to the given point.
    pub fn move_to(&mut self, to: Point<Pixels>) {
        self.contour_count += 1;
        self.start = to;
        self.current = to;
    }

    /// Draw a straight line from the current point to the given point.
    pub fn line_to(&mut self, to: Point<Pixels>) {
        self.contour_count += 1;
        if self.contour_count > 1 {
            self.push_triangle(
                (self.start, self.current, to),
                (point(0., 1.), point(0., 1.), point(0., 1.)),
            );
        }
        self.current = to;
    }

    /// Draw a curve from the current point to the given point, using the given control point.
    pub fn curve_to(&mut self, to: Point<Pixels>, ctrl: Point<Pixels>) {
        self.contour_count += 1;
        if self.contour_count > 1 {
            self.push_triangle(
                (self.start, self.current, to),
                (point(0., 1.), point(0., 1.), point(0., 1.)),
            );
        }

        self.push_triangle(
            (self.current, ctrl, to),
            (point(0., 0.), point(0.5, 0.), point(1., 1.)),
        );
        self.current = to;
    }

    /// Push a triangle to the Path.
    pub fn push_triangle(
        &mut self,
        xy: (Point<Pixels>, Point<Pixels>, Point<Pixels>),
        st: (Point<f32>, Point<f32>, Point<f32>),
    ) {
        self.bounds = self
            .bounds
            .union(&Bounds {
                origin: xy.0,
                size: Default::default(),
            })
            .union(&Bounds {
                origin: xy.1,
                size: Default::default(),
            })
            .union(&Bounds {
                origin: xy.2,
                size: Default::default(),
            });

        self.vertices.push(PathVertex {
            xy_position: xy.0,
            st_position: st.0,
            content_mask: Default::default(),
        });
        self.vertices.push(PathVertex {
            xy_position: xy.1,
            st_position: st.1,
            content_mask: Default::default(),
        });
        self.vertices.push(PathVertex {
            xy_position: xy.2,
            st_position: st.2,
            content_mask: Default::default(),
        });
    }
}

impl<T> Path<T>
where
    T: Clone + Debug + Default + PartialEq + PartialOrd + Add<T, Output = T> + Sub<Output = T>,
{
    #[allow(unused)]
    pub(crate) fn clipped_bounds(&self) -> Bounds<T> {
        self.bounds.intersect(&self.content_mask.bounds)
    }
}

impl From<Path<ScaledPixels>> for Primitive {
    fn from(path: Path<ScaledPixels>) -> Self {
        Primitive::Path(path)
    }
}

#[derive(Clone, Debug)]
#[repr(C)]
pub(crate) struct PathVertex<P: Clone + Debug + Default + PartialEq> {
    pub(crate) xy_position: Point<P>,
    pub(crate) st_position: Point<f32>,
    pub(crate) content_mask: ContentMask<P>,
}

impl PathVertex<Pixels> {
    pub fn scale(&self, factor: f32) -> PathVertex<ScaledPixels> {
        PathVertex {
            xy_position: self.xy_position.scale(factor),
            st_position: self.st_position,
            content_mask: self.content_mask.scale(factor),
        }
    }
}

#[cfg(test)]
mod backdrop_order_tests {
    use super::*;
    use crate::platform::{AtlasTextureKind, TileId};

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<ScaledPixels> {
        Bounds {
            origin: point(ScaledPixels(x), ScaledPixels(y)),
            size: Size {
                width: ScaledPixels(w),
                height: ScaledPixels(h),
            },
        }
    }

    fn mask(bounds: Bounds<ScaledPixels>) -> ContentMask<ScaledPixels> {
        ContentMask { bounds }
    }

    fn sprite(bounds: Bounds<ScaledPixels>, clip: Bounds<ScaledPixels>) -> MonochromeSprite {
        MonochromeSprite {
            order: 0,
            pad: 0,
            bounds,
            content_mask: mask(clip),
            color: Hsla::default(),
            tile: AtlasTile {
                texture_id: AtlasTextureId {
                    index: 0,
                    kind: AtlasTextureKind::Monochrome,
                },
                tile_id: TileId(0),
                padding: 0,
                bounds: Bounds::default(),
            },
            transformation: TransformationMatrix::default(),
        }
    }

    fn path(bounds: Bounds<ScaledPixels>, clip: Bounds<ScaledPixels>) -> Path<ScaledPixels> {
        let mut path = Path::new(point(Pixels(0.), Pixels(0.))).scale(1.);
        path.bounds = bounds;
        path.content_mask = mask(clip);
        path
    }

    fn glass(bounds: Bounds<ScaledPixels>, clip: Bounds<ScaledPixels>) -> Quad {
        Quad {
            bounds,
            content_mask: mask(clip),
            border_color: backdrop_blur_marker(1.),
            ..Default::default()
        }
    }

    fn kinds(scene: &Scene) -> Vec<&'static str> {
        scene
            .batches()
            .map(|batch| match batch {
                PrimitiveBatch::MonochromeSprites { .. } => "sprite",
                PrimitiveBatch::Paths(_) => "path",
                PrimitiveBatch::Quads(_) => "quad",
                _ => panic!("unexpected batch"),
            })
            .collect()
    }

    #[test]
    fn backdrop_markers_round_trip_and_scale_without_changing_coverage() {
        for marker in [backdrop_blur_marker, backdrop_blur_fade_marker] {
            for sigma in [0.25, 1., 12., BACKDROP_MAX_SIGMA] {
                let color = marker(sigma);
                assert!(is_backdrop_marker(&color)); // Also used by Style::paint.
                let quad = Quad {
                    border_color: color,
                    ..Default::default()
                };
                assert_eq!(backdrop_blur_sigma(&quad), Some(sigma));
                for scale in [0.5, 1., 2.] {
                    let scaled = scale_backdrop_marker(color, scale);
                    assert_eq!(scaled, marker((sigma * scale).min(BACKDROP_MAX_SIGMA)));
                    assert_eq!(
                        backdrop_blur_fades(&Quad {
                            border_color: scaled,
                            ..Default::default()
                        }),
                        backdrop_blur_fades(&quad)
                    );
                }
            }
            for sigma in [-1., 0.] {
                assert!(!is_backdrop_marker(&marker(sigma)));
            }
        }
        assert!(!backdrop_blur_fades(&Quad {
            border_color: backdrop_blur_marker(12.),
            ..Default::default()
        }));
        assert!(backdrop_blur_fades(&Quad {
            border_color: backdrop_blur_fade_marker(12.),
            ..Default::default()
        }));
        let ordinary = Hsla {
            h: 0.2,
            s: 0.3,
            l: 0.4,
            a: 0.,
        };
        assert!(!is_backdrop_marker(&ordinary));
        assert_eq!(scale_backdrop_marker(ordinary, 2.), ordinary);
        let visible = Hsla {
            a: 1.,
            ..backdrop_blur_fade_marker(12.)
        };
        assert!(!is_backdrop_marker(&visible));
    }

    #[test]
    fn fading_backdrop_barrier_survives_clipping_layers_and_replay() {
        let full = rect(-100., -100., 300., 300.);
        let mut source = Scene::default();
        source.push_layer(rect(50., 50., 2., 2.));
        source.insert_primitive(sprite(rect(14., 2., 1., 1.), full));
        source.pop_layer();
        let mut fade = glass(rect(0., 0., 10., 10.), rect(0., 5., 10., 5.));
        fade.border_color = backdrop_blur_fade_marker(1.);
        source.insert_primitive(fade);
        source.insert_primitive(path(rect(12., 2., 1., 1.), full));
        let mut replayed = Scene::default();
        replayed.replay(0..source.len(), &source);
        for scene in [&mut source, &mut replayed] {
            scene.finish();
            assert_eq!(kinds(scene), ["sprite", "quad", "path"]);
            assert!(backdrop_blur_fades(&scene.quads[0]));
            assert_eq!(scene.quads[0].bounds, rect(0., 0., 10., 10.));
            assert_eq!(scene.quads[0].content_mask.bounds, rect(0., 5., 10., 5.));
        }
    }

    #[test]
    fn backdrop_orders_surrounding_sprites_and_paths() {
        let clip = rect(-100., -100., 300., 300.);
        for use_path in [false, true] {
            let mut scene = Scene::default();
            let outside_glass = rect(14., 2., 1., 1.);
            if use_path {
                scene.insert_primitive(path(outside_glass, clip));
            } else {
                scene.insert_primitive(sprite(outside_glass, clip));
            }
            scene.insert_primitive(glass(rect(0., 0., 10., 10.), clip));
            scene.insert_primitive(path(rect(12., 2., 1., 1.), clip));
            scene.finish();
            assert_eq!(
                kinds(&scene),
                [if use_path { "path" } else { "sprite" }, "quad", "path"]
            );
        }
    }

    #[test]
    fn backdrop_uses_clipped_output_but_samples_outside_mask() {
        let clip = rect(5., 0., 5., 10.);
        let full = rect(-100., -100., 300., 300.);
        let mut scene = Scene::default();
        scene.insert_primitive(sprite(rect(1., 2., 1., 1.), full));
        scene.insert_primitive(glass(rect(0., 0., 10., 10.), clip));
        scene.finish();
        assert_eq!(kinds(&scene), ["sprite", "quad"]);

        // A fully clipped backdrop cannot impose a dependency.
        let mut scene = Scene::default();
        scene.insert_primitive(glass(rect(0., 0., 2., 2.), clip));
        assert_eq!(scene.len(), 0);
        // Fractional output bounds must include the renderer's rounded snapshot.
        assert_eq!(
            backdrop_blur_sample_bounds(rect(5.2, 1.2, 2., 2.), 1.),
            rect(0., -4., 13., 13.)
        );
    }

    #[test]
    fn layer_without_backdrop_preserves_batching() {
        let full = rect(-100., -100., 300., 300.);
        let mut scene = Scene::default();
        scene.push_layer(rect(0., 0., 20., 20.));
        for _ in 0..100 {
            scene.insert_primitive(sprite(rect(1., 1., 5., 5.), full));
        }
        scene.pop_layer();
        scene.finish();
        assert_eq!(kinds(&scene), ["sprite"]);
        assert!(
            scene
                .monochrome_sprites
                .iter()
                .all(|sprite| sprite.order == 1)
        );
        // Inserting another primitive over the layer should see only its reservation.
        scene.insert_primitive(path(rect(1., 1., 1., 1.), full));
        assert_eq!(scene.paths[0].order, 2);
    }

    #[test]
    fn backdrop_respects_draws_inside_same_layer() {
        let full = rect(-100., -100., 300., 300.);
        let mut scene = Scene::default();
        scene.push_layer(rect(0., 0., 20., 20.));
        scene.insert_primitive(sprite(rect(14., 2., 1., 1.), full));
        scene.insert_primitive(glass(rect(0., 0., 10., 10.), full));
        scene.insert_primitive(path(rect(12., 2., 1., 1.), full));
        scene.pop_layer();
        scene.finish();
        assert_eq!(kinds(&scene), ["sprite", "quad", "path"]);
    }

    #[test]
    fn backdrop_barrier_crosses_nested_and_closed_layers_and_replay() {
        let full = rect(-100., -100., 300., 300.);
        let mut source = Scene::default();
        // The earlier primitive is outside both layer reservations, but inside
        // the later backdrop's expanded sample region.
        source.push_layer(rect(50., 50., 2., 2.));
        source.insert_primitive(sprite(rect(14., 2., 1., 1.), full));
        source.pop_layer();
        source.push_layer(rect(0., 0., 10., 10.));
        source.push_layer(rect(0., 0., 10., 10.));
        source.insert_primitive(glass(rect(0., 0., 10., 10.), full));
        source.pop_layer();
        source.insert_primitive(path(rect(12., 2., 1., 1.), full));
        source.pop_layer();
        source.push_layer(rect(60., 60., 2., 2.));
        source.insert_primitive(sprite(rect(13., 2., 1., 1.), full));
        source.pop_layer();
        let mut replayed = Scene::default();
        replayed.replay(0..source.len(), &source);
        for scene in [&mut source, &mut replayed] {
            scene.finish();
            assert_eq!(kinds(scene), ["sprite", "quad", "path", "sprite"]);
        }
    }

    #[test]
    fn nested_layers_follow_post_backdrop_parent_draws_and_replay() {
        let full = rect(-100., -100., 300., 300.);
        let overlap = rect(40., 40., 2., 2.);
        let mut source = Scene::default();
        source.push_layer(rect(0., 0., 10., 10.));
        source.insert_primitive(glass(rect(0., 0., 2., 2.), full));
        // Both draws extend beyond their layers' reservations.
        source.insert_primitive(sprite(overlap, full));
        source.push_layer(rect(20., 20., 2., 2.));
        source.insert_primitive(path(overlap, full));
        source.pop_layer();
        source.insert_primitive(sprite(overlap, full));
        source.push_layer(rect(30., 30., 2., 2.));
        source.insert_primitive(path(overlap, full));
        source.pop_layer();
        source.pop_layer();
        let mut replayed = Scene::default();
        replayed.replay(0..source.len(), &source);
        for scene in [&mut source, &mut replayed] {
            scene.finish();
            assert_eq!(kinds(scene), ["quad", "sprite", "path", "sprite", "path"]);
        }
    }

    #[test]
    fn post_backdrop_top_level_draw_precedes_disjoint_layer_reservation() {
        let full = rect(-100., -100., 300., 300.);
        let overlap = rect(40., 40., 2., 2.);
        let mut source = Scene::default();
        source.insert_primitive(glass(rect(0., 0., 2., 2.), full));
        source.insert_primitive(sprite(overlap, full));
        source.push_layer(rect(20., 20., 2., 2.));
        source.insert_primitive(path(overlap, full));
        source.pop_layer();
        let mut replayed = Scene::default();
        replayed.replay(0..source.len(), &source);
        for scene in [&mut source, &mut replayed] {
            scene.finish();
            assert_eq!(kinds(scene), ["quad", "sprite", "path"]);
        }
    }

    #[test]
    fn clear_restores_no_backdrop_layer_batching() {
        let full = rect(-100., -100., 300., 300.);
        let mut scene = Scene::default();
        scene.insert_primitive(glass(rect(0., 0., 2., 2.), full));
        scene.clear();
        scene.push_layer(rect(0., 0., 10., 10.));
        scene.insert_primitive(sprite(rect(1., 1., 1., 1.), full));
        scene.push_layer(rect(20., 20., 2., 2.));
        scene.insert_primitive(sprite(rect(20., 20., 1., 1.), full));
        scene.pop_layer();
        scene.pop_layer();
        scene.finish();
        assert_eq!(kinds(&scene), ["sprite"]);
        assert!(
            scene
                .monochrome_sprites
                .iter()
                .all(|sprite| sprite.order == 1)
        );
    }

    #[test]
    fn post_backdrop_draws_do_not_escape_an_old_layer_reservation() {
        let full = rect(-100., -100., 300., 300.);
        let mut scene = Scene::default();
        scene.push_layer(rect(0., 0., 20., 20.));
        scene.insert_primitive(glass(rect(0., 0., 2., 2.), full));
        scene.insert_primitive(sprite(rect(15., 15., 1., 1.), full));
        scene.pop_layer();
        scene.insert_primitive(path(rect(15., 15., 1., 1.), full));
        scene.finish();
        assert_eq!(kinds(&scene), ["quad", "sprite", "path"]);
    }

    #[test]
    fn backdrop_dependencies_survive_layers_and_replay() {
        let full = rect(-100., -100., 300., 300.);
        let mut source = Scene::default();
        source.insert_primitive(sprite(rect(14., 2., 1., 1.), full));
        source.push_layer(rect(0., 0., 10., 10.));
        source.insert_primitive(glass(rect(0., 0., 10., 10.), full));
        source.insert_primitive(path(rect(12., 2., 1., 1.), full));
        source.pop_layer();
        let mut replayed = Scene::default();
        replayed.replay(0..source.len(), &source);
        for scene in [&mut source, &mut replayed] {
            scene.finish();
            assert_eq!(kinds(scene), ["sprite", "quad", "path"]);
        }
    }
}
