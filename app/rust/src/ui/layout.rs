//! Owned by WP-10a: declarative form trees and the WinForms layout subset the forms use (A9).
//!
//! A form is a tree of [`Node`]s: containers (`Panel` = DefaultLayout dock, `Table` =
//! TableLayoutPanel, `Flow` = FlowLayoutPanel) and leaves (one native control each). The tree is
//! data; [`arrange`] places every node exactly like the WinForms engines, which this file ports.
//!
//! Units: builders take 96-DPI logical pixels. The window kit calls [`Node::rescale`] once at
//! creation and on every DPI change (like WinForms `ScaleControl`), so a live tree holds device
//! pixels. Code that edits a live tree must use device pixels (`Form::scale`).
//!
//! Differences from WinForms, on purpose: Panel children dock in list order (WinForms docks in
//! reverse `Controls` order, so C# adds the bottom control first); table cells always carry an
//! explicit column and row; no cell borders, no row spans, no right-to-left containers.
//!
//! Portions ported from dotnet/winforms `Layout/FlowLayout*.cs`, `Layout/TableLayout.cs`,
//! `Layout/DefaultLayout.cs`, `Layout/LayoutUtils.cs`, `Control.cs` (`GetPreferredSize`):
//!
//! The MIT License (MIT)
//!
//! Copyright (c) .NET Foundation and Contributors
//!
//! All rights reserved.
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.

use super::controls::Ctl;
use super::theme::{self, Color};

const UNBOUNDED: i32 = i32::MAX;

/// Width and height.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Size {
    /// Width.
    pub w: i32,
    /// Height.
    pub h: i32,
}

/// A position.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Point {
    /// Horizontal position.
    pub x: i32,
    /// Vertical position.
    pub y: i32,
}

/// WinForms `Padding`: left, top, right, bottom.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Pad {
    /// Left.
    pub l: i32,
    /// Top.
    pub t: i32,
    /// Right.
    pub r: i32,
    /// Bottom.
    pub b: i32,
}

impl Pad {
    /// Left + right.
    pub fn horizontal(self) -> i32 {
        self.l + self.r
    }

    /// Top + bottom.
    pub fn vertical(self) -> i32 {
        self.t + self.b
    }

    fn size(self) -> Size {
        Size {
            w: self.horizontal(),
            h: self.vertical(),
        }
    }

    fn flip(self) -> Pad {
        Pad {
            l: self.t,
            t: self.l,
            r: self.b,
            b: self.r,
        }
    }
}

/// A rectangle as position plus size.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Rect {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width.
    pub w: i32,
    /// Height.
    pub h: i32,
}

impl Rect {
    /// Right edge (exclusive).
    pub fn right(self) -> i32 {
        self.x.saturating_add(self.w)
    }

    /// Bottom edge (exclusive).
    pub fn bottom(self) -> i32 {
        self.y.saturating_add(self.h)
    }

    /// `LayoutUtils.DeflateRect`.
    pub fn deflate(self, p: Pad) -> Rect {
        Rect {
            x: self.x + p.l,
            y: self.y + p.t,
            w: self.w - p.horizontal(),
            h: self.h - p.vertical(),
        }
    }

    /// The size part.
    pub fn size(self) -> Size {
        Size {
            w: self.w,
            h: self.h,
        }
    }

    fn flip(self) -> Rect {
        Rect {
            x: self.y,
            y: self.x,
            w: self.h,
            h: self.w,
        }
    }
}

impl Size {
    fn flip(self) -> Size {
        Size {
            w: self.h,
            h: self.w,
        }
    }

    fn union(self, o: Size) -> Size {
        Size {
            w: self.w.max(o.w),
            h: self.h.max(o.h),
        }
    }
}

/// `TableLayoutPanel` column or row style.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Track {
    /// `SizeType.Absolute` (pixels).
    Absolute(i32),
    /// `SizeType.Percent`.
    Percent(f32),
    /// `SizeType.AutoSize`.
    AutoSize,
}

/// `DockStyle` subset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Dock {
    /// Not docked: anchored.
    None,
    /// `DockStyle.Top`.
    Top,
    /// `DockStyle.Fill`.
    Fill,
}

/// `AnchorStyles` bit set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Anchor(pub u8);

impl Anchor {
    /// `AnchorStyles.None`: centered in a table cell.
    pub const NONE: Anchor = Anchor(0);
    /// `AnchorStyles.Top`.
    pub const TOP: Anchor = Anchor(1);
    /// `AnchorStyles.Bottom`.
    pub const BOTTOM: Anchor = Anchor(2);
    /// `AnchorStyles.Left`.
    pub const LEFT: Anchor = Anchor(4);
    /// `AnchorStyles.Right`.
    pub const RIGHT: Anchor = Anchor(8);
    /// WinForms default `Top | Left`.
    pub const DEFAULT: Anchor = Anchor(1 | 4);

    fn has(self, other: Anchor) -> bool {
        self.0 & other.0 != 0
    }

    fn vertical(self) -> u8 {
        self.0 & (Self::TOP.0 | Self::BOTTOM.0)
    }

    fn horizontal(self) -> u8 {
        self.0 & (Self::LEFT.0 | Self::RIGHT.0)
    }
}

/// `FlowDirection` subset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlowDir {
    /// `FlowDirection.LeftToRight`.
    LeftToRight,
    /// `FlowDirection.RightToLeft`.
    RightToLeft,
    /// `FlowDirection.TopDown`.
    TopDown,
}

/// What a node is.
#[derive(Debug)]
pub enum Kind {
    /// One native control.
    Leaf(Ctl),
    /// `Panel`: children dock (Top, Fill) in list order; undocked children sit at `pos`.
    Panel(Vec<Node>),
    /// `TableLayoutPanel`; each child sets `col`, `row`, and optionally `col_span`.
    Table {
        /// Column styles.
        cols: Vec<Track>,
        /// Row styles.
        rows: Vec<Track>,
        /// Cell contents.
        cells: Vec<Node>,
    },
    /// `FlowLayoutPanel`.
    Flow {
        /// Flow direction.
        dir: FlowDir,
        /// `WrapContents`.
        wrap: bool,
        /// Children in add order.
        children: Vec<Node>,
    },
}

/// One control or container with its WinForms layout properties.
#[derive(Debug)]
pub struct Node {
    /// Control id for leaves (form-unique, non-zero); optional for containers (0 = none).
    pub id: u16,
    /// Leaf or container.
    pub kind: Kind,
    /// `Margin`.
    pub margin: Pad,
    /// `Padding`.
    pub padding: Pad,
    /// `Dock`.
    pub dock: Dock,
    /// `Anchor` (ignored when docked).
    pub anchor: Anchor,
    /// `AutoSize`.
    pub auto_size: bool,
    /// Specified size (`Size`).
    pub size: Size,
    /// `MinimumSize`.
    pub min: Size,
    /// `MaximumSize` (0 = unbounded).
    pub max: Size,
    /// `Location` for undocked children of a `Panel`.
    pub pos: Point,
    /// Table column.
    pub col: usize,
    /// Table row.
    pub row: usize,
    /// Table column span.
    pub col_span: usize,
    /// `BackColor`; `None` = inherit from the parent (WinForms ambient color).
    pub back: Option<Color>,
    /// `Visible`.
    pub visible: bool,
    /// `AutoScroll` (vertical only).
    pub scroll: bool,
    /// Live: bounds relative to the parent container's client area (device pixels).
    pub bounds: Rect,
    /// Live: vertical scroll position of a scroll container.
    pub scroll_pos: i32,
    /// Live: content height of a scroll container (device pixels).
    pub content_height: i32,
    /// Live: whether the vertical scroll bar is needed.
    pub vscroll: bool,
    /// Live: scroll bar width the kit sets before layout (device pixels).
    pub scroll_bar_width: i32,
    /// The 96-DPI values, captured on the first `rescale`.
    logical: Option<Box<Logical>>,
}

/// The logical (96-DPI) lengths of a node, the source of every rescale.
#[derive(Clone, Debug)]
struct Logical {
    margin: Pad,
    padding: Pad,
    size: Size,
    min: Size,
    max: Size,
    pos: Point,
    tracks: Vec<Track>,
}

impl Node {
    fn new(id: u16, kind: Kind, margin: Pad, size: Size) -> Self {
        Self {
            id,
            kind,
            margin,
            padding: theme::NO_PAD,
            dock: Dock::None,
            anchor: Anchor::DEFAULT,
            auto_size: false,
            size,
            min: Size::default(),
            max: Size::default(),
            pos: Point::default(),
            col: 0,
            row: 0,
            col_span: 1,
            back: None,
            visible: true,
            scroll: false,
            bounds: Rect::default(),
            scroll_pos: 0,
            content_height: 0,
            vscroll: false,
            scroll_bar_width: 0,
            logical: None,
        }
    }

    /// A leaf control with the WinForms default margin and size of its kind.
    pub fn leaf(id: u16, ctl: Ctl) -> Self {
        let (margin, size) = ctl.defaults();
        Self::new(id, Kind::Leaf(ctl), margin, size)
    }

    /// A `Panel` with docked or positioned children.
    pub fn panel(children: Vec<Node>) -> Self {
        Self::new(
            0,
            Kind::Panel(children),
            theme::DEFAULT_MARGIN,
            theme::PANEL_DEFAULT_SIZE,
        )
    }

    /// A `TableLayoutPanel`.
    pub fn table(cols: Vec<Track>, rows: Vec<Track>, cells: Vec<Node>) -> Self {
        Self::new(
            0,
            Kind::Table { cols, rows, cells },
            theme::DEFAULT_MARGIN,
            theme::PANEL_DEFAULT_SIZE,
        )
    }

    /// A `FlowLayoutPanel`.
    pub fn flow(dir: FlowDir, wrap: bool, children: Vec<Node>) -> Self {
        Self::new(
            0,
            Kind::Flow {
                dir,
                wrap,
                children,
            },
            theme::DEFAULT_MARGIN,
            theme::PANEL_DEFAULT_SIZE,
        )
    }

    /// Sets the container id (for `Form` lookups).
    pub fn id(mut self, id: u16) -> Self {
        self.id = id;
        self
    }

    /// Sets `Margin`.
    pub fn margin(mut self, margin: Pad) -> Self {
        self.margin = margin;
        self
    }

    /// Sets `Padding`.
    pub fn padding(mut self, padding: Pad) -> Self {
        self.padding = padding;
        self
    }

    /// `Dock = DockStyle.Fill`.
    pub fn fill(mut self) -> Self {
        self.dock = Dock::Fill;
        self
    }

    /// `Dock = DockStyle.Top`.
    pub fn top(mut self) -> Self {
        self.dock = Dock::Top;
        self
    }

    /// Sets `Anchor`.
    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// `AutoSize = true`.
    pub fn auto_size(mut self) -> Self {
        self.auto_size = true;
        self
    }

    /// Sets the specified `Size`.
    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    /// Sets the specified height only.
    pub fn height(mut self, h: i32) -> Self {
        self.size.h = h;
        self
    }

    /// Sets `MinimumSize`.
    pub fn min(mut self, min: Size) -> Self {
        self.min = min;
        self
    }

    /// Sets `MaximumSize` (0 = unbounded).
    pub fn max(mut self, max: Size) -> Self {
        self.max = max;
        self
    }

    /// Sets `Location` (undocked child of a `Panel`).
    pub fn pos(mut self, pos: Point) -> Self {
        self.pos = pos;
        self
    }

    /// Places the node in table cell (`col`, `row`).
    pub fn cell(mut self, col: usize, row: usize) -> Self {
        self.col = col;
        self.row = row;
        self
    }

    /// Sets the table column span.
    pub fn span(mut self, cols: usize) -> Self {
        self.col_span = cols.max(1);
        self
    }

    /// Sets `BackColor`.
    pub fn back(mut self, color: Color) -> Self {
        self.back = Some(color);
        self
    }

    /// `AutoScroll = true` (vertical scroll bar when the content is taller than the client).
    pub fn scroll(mut self) -> Self {
        self.scroll = true;
        self
    }

    /// Sets `Visible`.
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Child nodes of a container (empty for leaves).
    pub fn children(&self) -> &[Node] {
        match &self.kind {
            Kind::Leaf(_) => &[],
            Kind::Panel(c) => c,
            Kind::Table { cells, .. } => cells,
            Kind::Flow { children, .. } => children,
        }
    }

    /// Mutable child nodes of a container.
    pub fn children_mut(&mut self) -> &mut [Node] {
        match &mut self.kind {
            Kind::Leaf(_) => &mut [],
            Kind::Panel(c) => c,
            Kind::Table { cells, .. } => cells,
            Kind::Flow { children, .. } => children,
        }
    }

    /// Finds the node with `id` in this subtree.
    pub fn find(&self, id: u16) -> Option<&Node> {
        if self.id == id {
            return Some(self);
        }
        self.children().iter().find_map(|c| c.find(id))
    }

    /// Finds the node with `id` in this subtree, mutably.
    pub fn find_mut(&mut self, id: u16) -> Option<&mut Node> {
        if self.id == id {
            return Some(self);
        }
        self.children_mut().iter_mut().find_map(|c| c.find_mut(id))
    }

    /// Whether this node is a leaf control.
    pub fn is_leaf(&self) -> bool {
        matches!(self.kind, Kind::Leaf(_))
    }

    /// Rescales every length in the subtree to `to` DPI (WinForms `ScaleControl`).
    ///
    /// Lengths are computed from the logical (96-DPI) values captured on the first call, so
    /// moving between monitors never accumulates rounding (WinForms scales the current values
    /// and can drift by a pixel per move). Device-pixel edits made through `Form::with_tree`
    /// are replaced too; re-apply them in the `Event::Resize` handler that follows.
    pub fn rescale(&mut self, from: u32, to: u32) {
        if self.logical.is_none() {
            self.logical = Some(Box::new(Logical {
                margin: self.margin,
                padding: self.padding,
                size: self.size,
                min: self.min,
                max: self.max,
                pos: self.pos,
                tracks: match &self.kind {
                    Kind::Table { cols, rows, .. } => cols.iter().chain(rows).copied().collect(),
                    _ => Vec::new(),
                },
            }));
        }
        if from != to {
            self.scroll_pos = (f64::from(self.scroll_pos) * f64::from(to) / f64::from(from))
                .round_ties_even() as i32;
        }
        let Some(base) = self.logical.as_deref().cloned() else {
            return;
        };
        let s = |v: i32| {
            if v == 0 || v == UNBOUNDED {
                v
            } else {
                super::dpi::scale(v, to)
            }
        };
        let sp = |p: Pad| Pad {
            l: s(p.l),
            t: s(p.t),
            r: s(p.r),
            b: s(p.b),
        };
        let ss = |z: Size| Size {
            w: s(z.w),
            h: s(z.h),
        };
        self.margin = sp(base.margin);
        self.padding = sp(base.padding);
        self.size = ss(base.size);
        self.min = ss(base.min);
        self.max = ss(base.max);
        self.pos = Point {
            x: s(base.pos.x),
            y: s(base.pos.y),
        };
        if let Kind::Table { cols, rows, .. } = &mut self.kind {
            for (t, b) in cols.iter_mut().chain(rows.iter_mut()).zip(&base.tracks) {
                *t = match *b {
                    Track::Absolute(v) => Track::Absolute(s(v)),
                    other => other,
                };
            }
        }
        for c in self.children_mut() {
            c.rescale(from, to);
        }
    }
}

/// Measures an auto-sized leaf: `(node, proposed size) -> preferred size` in device pixels.
pub type Measure<'a> = dyn FnMut(&Node, Size) -> Size + 'a;

/// Lays out `node` at `bounds` (relative to its parent's client area) and everything under it.
pub fn arrange(node: &mut Node, bounds: Rect, measure: &mut Measure<'_>) {
    node.bounds = bounds;
    if node.is_leaf() {
        return;
    }
    let client = Rect {
        x: 0,
        y: 0,
        w: bounds.w,
        h: bounds.h,
    };
    let mut display = client.deflate(node.padding);
    if node.scroll {
        // ScrollableControl.AutoScroll: measure the content at the client width; a vertical
        // scroll bar takes width away and the content is measured again.
        node.vscroll = false;
        let mut pref = core_size(node, Size { w: display.w, h: 0 }, measure);
        if pref.h + node.padding.vertical() > client.h {
            node.vscroll = true;
            display.w -= node.scroll_bar_width;
            pref = core_size(
                node,
                Size {
                    w: display.w.max(1),
                    h: 0,
                },
                measure,
            );
        }
        node.content_height = pref.h + node.padding.vertical();
        let max_pos = (node.content_height - client.h).max(0);
        node.scroll_pos = node.scroll_pos.clamp(0, max_pos);
        display.y -= node.scroll_pos;
        display.h = display.h.max(pref.h);
    }
    match node.kind {
        Kind::Leaf(_) => {}
        Kind::Panel(_) => arrange_panel(node, display, measure),
        Kind::Table { .. } => arrange_table(node, display, measure),
        Kind::Flow { .. } => flow_calculate(node, display, measure),
    }
}

/// `Control.GetPreferredSize`: zero means unbounded; result clamped to `MinimumSize`/`MaximumSize`.
pub fn preferred(node: &Node, proposed: Size, measure: &mut Measure<'_>) -> Size {
    let proposed = constrain(node, unbounded_zero(proposed));
    let core = match node.kind {
        Kind::Leaf(_) => measure(node, proposed),
        _ => {
            let pad = node.padding.size();
            let inner = Size {
                w: proposed.w.saturating_sub(pad.w),
                h: proposed.h.saturating_sub(pad.h),
            };
            let s = core_size(node, inner, measure);
            Size {
                w: s.w + pad.w,
                h: s.h + pad.h,
            }
        }
    };
    constrain(node, core)
}

/// `TableLayout.GetElementSize`: preferred size when auto-sized, otherwise the specified size.
fn element_size(node: &Node, proposed: Size, measure: &mut Measure<'_>) -> Size {
    if node.auto_size {
        preferred(node, proposed, measure)
    } else {
        node.size
    }
}

fn unbounded_zero(s: Size) -> Size {
    Size {
        w: if s.w == 0 { UNBOUNDED } else { s.w },
        h: if s.h == 0 { UNBOUNDED } else { s.h },
    }
}

/// `Control.ApplySizeConstraints`.
fn constrain(node: &Node, s: Size) -> Size {
    if node.max == Size::default() && node.min == Size::default() {
        return s;
    }
    let max = unbounded_zero(node.max);
    Size {
        w: s.w.min(max.w).max(node.min.w),
        h: s.h.min(max.h).max(node.min.h),
    }
}

/// The container engine's preferred size for an inner (padding-free) proposal.
fn core_size(node: &Node, proposed: Size, measure: &mut Measure<'_>) -> Size {
    match node.kind {
        Kind::Leaf(_) => Size::default(),
        Kind::Panel(_) => panel_preferred(node, measure),
        Kind::Table { .. } => table_preferred(node, proposed, measure),
        Kind::Flow { .. } => flow_preferred(node, proposed, measure),
    }
}

fn unified_anchor(node: &Node) -> Anchor {
    match node.dock {
        Dock::None => node.anchor,
        Dock::Top => Anchor(Anchor::TOP.0 | Anchor::LEFT.0 | Anchor::RIGHT.0),
        Dock::Fill => Anchor(15),
    }
}

/// `LayoutUtils.AlignAndStretch`.
fn align_and_stretch(fit: Size, within: Rect, a: Anchor) -> Rect {
    let mut s = Size {
        w: if a.horizontal() == Anchor::LEFT.0 | Anchor::RIGHT.0 {
            within.w
        } else {
            fit.w
        },
        h: if a.vertical() == Anchor::TOP.0 | Anchor::BOTTOM.0 {
            within.h
        } else {
            fit.h
        },
    };
    s.w = s.w.min(within.w);
    s.h = s.h.min(within.h);
    let mut r = within;
    if a.has(Anchor::RIGHT) {
        r.x += within.w - s.w;
    } else if a.horizontal() == 0 {
        r.x += (within.w - s.w) / 2;
    }
    if a.has(Anchor::BOTTOM) {
        r.y += within.h - s.h;
    } else if a.vertical() == 0 {
        r.y += (within.h - s.h) / 2;
    }
    r.w = s.w;
    r.h = s.h;
    r
}

// ---------------------------------------------------------------------------------------------
// DefaultLayout (Panel): Dock Top / Fill, undocked children at their location.
// ---------------------------------------------------------------------------------------------

fn docked_size(child: &Node, constraints: Size, measure: &mut Measure<'_>) -> Size {
    if child.auto_size {
        preferred(child, constraints, measure)
    } else {
        child.size
    }
}

fn arrange_panel(node: &mut Node, display: Rect, measure: &mut Measure<'_>) {
    let mut remaining = display;
    for child in node.children_mut().iter_mut().filter(|c| c.visible) {
        match child.dock {
            Dock::Top => {
                let size = docked_size(
                    child,
                    Size {
                        w: remaining.w,
                        h: 1,
                    },
                    measure,
                );
                let h = constrain(
                    child,
                    Size {
                        w: remaining.w,
                        h: size.h,
                    },
                )
                .h;
                let r = Rect {
                    x: remaining.x,
                    y: remaining.y,
                    w: remaining.w,
                    h,
                };
                arrange(child, r, measure);
                remaining.y += h;
                remaining.h -= h;
            }
            Dock::Fill => arrange(child, remaining, measure),
            Dock::None => {
                let size = if child.auto_size {
                    preferred(child, Size::default(), measure)
                } else {
                    child.size
                };
                let r = Rect {
                    x: display.x + child.pos.x,
                    y: display.y + child.pos.y,
                    w: size.w,
                    h: size.h,
                };
                arrange(child, r, measure);
            }
        }
    }
}

/// `DefaultLayout.TryCalculatePreferredSize(measureOnly: true)`.
fn panel_preferred(node: &Node, measure: &mut Measure<'_>) -> Size {
    let mut docked = Size::default();
    let mut remaining = Size::default();
    let mut anchored = Size::default();
    for child in node.children().iter().filter(|c| c.visible) {
        match child.dock {
            Dock::Top => {
                let size = docked_size(
                    child,
                    Size {
                        w: remaining.w,
                        h: 1,
                    },
                    measure,
                );
                let needed = (size.h - remaining.h).max(0);
                docked.h += needed;
                remaining.h += needed;
                // DefaultLayout.LayoutDockedControls: the element's height leaves the rest.
                remaining.h -= size.h;
            }
            Dock::Fill => {
                if child.auto_size {
                    let p = preferred(child, Size::default(), measure);
                    remaining.w += p.w;
                    remaining.h += p.h;
                    docked.w += p.w;
                    docked.h += p.h;
                }
            }
            Dock::None => {
                let size = if child.auto_size {
                    preferred(child, Size::default(), measure)
                } else {
                    child.size
                };
                anchored.w = anchored.w.max(child.pos.x + size.w + child.margin.r);
                anchored.h = anchored.h.max(child.pos.y + size.h + child.margin.b);
            }
        }
    }
    docked.union(anchored)
}

// ---------------------------------------------------------------------------------------------
// FlowLayout
// ---------------------------------------------------------------------------------------------

/// The flow algorithm in "horizontal" coordinates; TopDown flips every size on the way in and
/// every rectangle on the way out (`FlowLayout.VerticalElementProxy`).
struct FlowEl {
    margin: Pad,
    min: Size,
    specified: Size,
    auto: bool,
    stretches: bool,
    anchor: Anchor,
}

fn flow_el(child: &Node, vertical: bool) -> FlowEl {
    let unified = unified_anchor(child);
    let (stretch_bits, near, far) = if vertical {
        (
            Anchor::LEFT.0 | Anchor::RIGHT.0,
            Anchor::LEFT,
            Anchor::RIGHT,
        )
    } else {
        (
            Anchor::TOP.0 | Anchor::BOTTOM.0,
            Anchor::TOP,
            Anchor::BOTTOM,
        )
    };
    let anchor = if unified.0 & stretch_bits == stretch_bits {
        Anchor(Anchor::TOP.0 | Anchor::BOTTOM.0)
    } else if unified.has(near) {
        Anchor::TOP
    } else if unified.has(far) {
        Anchor::BOTTOM
    } else {
        Anchor::NONE
    };
    let flip_size = |s: Size| if vertical { s.flip() } else { s };
    FlowEl {
        margin: if vertical {
            child.margin.flip()
        } else {
            child.margin
        },
        min: flip_size(child.min),
        specified: flip_size(child.size),
        auto: child.auto_size,
        stretches: anchor.vertical() == Anchor::TOP.0 | Anchor::BOTTOM.0,
        anchor,
    }
}

fn flow_params(node: &Node) -> (FlowDir, bool) {
    match node.kind {
        Kind::Flow { dir, wrap, .. } => (dir, wrap),
        _ => (FlowDir::LeftToRight, true),
    }
}

/// `FlowLayout.GetPreferredSize` (inner size, without the container padding).
fn flow_preferred(node: &Node, proposed: Size, measure: &mut Measure<'_>) -> Size {
    let mut bounds = Rect {
        x: 0,
        y: 0,
        w: proposed.w,
        h: proposed.h,
    };
    let pref = flow_measure(node, bounds, measure);
    if pref.w > proposed.w || pref.h > proposed.h {
        bounds.w = pref.w;
        bounds.h = pref.h;
        return flow_measure(node, bounds, measure);
    }
    pref
}

fn flow_measure(node: &Node, display: Rect, measure: &mut Measure<'_>) -> Size {
    // Measuring never writes bounds; a scratch walk over an immutable tree.
    let (dir, wrap) = flow_params(node);
    let vertical = dir == FlowDir::TopDown;
    let mut display = if vertical { display.flip() } else { display };
    if !wrap {
        display.w = UNBOUNDED - display.x;
    }
    let children = node.children();
    let mut layout = Size::default();
    let mut i = 0;
    while i < children.len() {
        let row = Rect {
            x: display.x,
            y: display.y,
            w: display.w,
            h: display.h.saturating_sub(layout.h),
        };
        let (row_size, brk) = flow_row(children, vertical, i, row, measure);
        layout.w = layout.w.max(row_size.w);
        layout.h += row_size.h;
        if brk == i {
            break;
        }
        i = brk;
    }
    if vertical { layout.flip() } else { layout }
}

/// `FlowLayout.TryCalculatePreferredSizeRow(measureOnly: true)`: row size and break index.
fn flow_row(
    children: &[Node],
    vertical: bool,
    start: usize,
    row: Rect,
    measure: &mut Measure<'_>,
) -> (Size, usize) {
    let mut x = row.x;
    let mut size = Size::default();
    let mut laid_out = 0;
    let mut brk = start;
    for (i, child) in children.iter().enumerate().skip(start) {
        if !child.visible {
            brk = i + 1;
            continue;
        }
        let el = flow_el(child, vertical);
        let pref = flow_pref(child, &el, vertical, row, size, i == start, measure);
        let required = Size {
            w: pref.w + el.margin.horizontal(),
            h: pref.h + el.margin.vertical(),
        };
        x = x.saturating_add(required.w);
        if laid_out > 0 && x > row.right() {
            return (size, i);
        }
        size.w = x - row.x;
        size.h = size.h.max(required.h);
        laid_out += 1;
        brk = i + 1;
    }
    (size, brk)
}

fn flow_pref(
    child: &Node,
    el: &FlowEl,
    vertical: bool,
    row: Rect,
    row_size: Size,
    first: bool,
    measure: &mut Measure<'_>,
) -> Size {
    if el.auto {
        let mut c = Size {
            w: UNBOUNDED,
            h: row.h.saturating_sub(el.margin.vertical()),
        };
        if first {
            c.w = row.w - row_size.w - el.margin.horizontal();
        }
        c = Size { w: 1, h: 1 }.union(c);
        if vertical {
            preferred(child, c.flip(), measure).flip()
        } else {
            preferred(child, c, measure)
        }
    } else {
        let mut p = el.specified;
        if el.stretches {
            p.h = 0;
        }
        if p.h < el.min.h {
            p.h = el.min.h;
        }
        p
    }
}

/// `FlowLayout.TryCalculatePreferredSize(measureOnly: false)`: places every child.
fn flow_calculate(node: &mut Node, display: Rect, measure: &mut Measure<'_>) {
    let (dir, wrap) = flow_params(node);
    let vertical = dir == FlowDir::TopDown;
    let container_display = if vertical { display.flip() } else { display };
    let mut row_display = container_display;
    if !wrap {
        row_display.w = UNBOUNDED - row_display.x;
    }
    let mut layout = Size::default();
    let mut i = 0;
    let count = node.children().len();
    while i < count {
        let measure_bounds = Rect {
            x: row_display.x,
            y: row_display.y,
            w: row_display.w,
            h: row_display.h.saturating_sub(layout.h),
        };
        let (row_size, brk) = flow_row(node.children(), vertical, i, measure_bounds, measure);
        let row_bounds = Rect {
            x: row_display.x,
            y: layout.h + row_display.y,
            w: row_size.w,
            h: row_size.h,
        };
        flow_place_row(
            node.children_mut(),
            dir,
            container_display,
            i,
            brk,
            row_bounds,
            measure,
        );
        layout.w = layout.w.max(row_size.w);
        layout.h += row_size.h;
        if brk == i {
            break;
        }
        i = brk;
    }
}

/// `FlowLayout.LayoutRow`: same walk as `flow_row`, but sets bounds.
fn flow_place_row(
    children: &mut [Node],
    dir: FlowDir,
    container_display: Rect,
    start: usize,
    end: usize,
    row: Rect,
    measure: &mut Measure<'_>,
) {
    let vertical = dir == FlowDir::TopDown;
    let mut x = row.x;
    let mut row_size = Size::default();
    for (i, child) in children.iter_mut().enumerate().take(end).skip(start) {
        if !child.visible {
            continue;
        }
        let el = flow_el(child, vertical);
        let pref = flow_pref(child, &el, vertical, row, row_size, i == start, measure);
        let required = Size {
            w: pref.w + el.margin.horizontal(),
            h: pref.h + el.margin.vertical(),
        };
        let cell = Rect {
            x,
            y: row.y,
            w: required.w,
            h: row.h,
        }
        .deflate(el.margin);
        let mut r = align_and_stretch(pref, cell, el.anchor);
        if dir == FlowDir::RightToLeft {
            // C# parity: FlowLayout.ContainerProxy.RTLTranslateNoMarginSwap.
            r.x = container_display.right() - r.x - r.w + el.margin.l - el.margin.r;
        }
        if vertical {
            r = r.flip();
        }
        let r = Rect {
            w: r.w.max(0),
            h: r.h.max(0),
            ..r
        };
        arrange(child, r, measure);
        x = x.saturating_add(required.w);
        row_size.w = x - row.x;
        row_size.h = row_size.h.max(required.h);
    }
}

// ---------------------------------------------------------------------------------------------
// TableLayout
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
struct Strip {
    min: i32,
    max: i32,
    is_start: bool,
}

struct TableInfo<'a> {
    cols: &'a [Track],
    rows: &'a [Track],
    col_strips: Vec<Strip>,
    row_strips: Vec<Strip>,
    /// Indexes of participating cells, sorted for the current pass.
    cells: Vec<usize>,
}

fn is_absolute(styles: &[Track], i: usize) -> bool {
    matches!(styles.get(i), Some(Track::Absolute(_)))
}

fn init_strips(styles: &[Track], strips: &mut [Strip]) {
    for (i, strip) in strips.iter_mut().enumerate() {
        let v = match styles.get(i) {
            Some(Track::Absolute(v)) => *v,
            _ => 0,
        };
        *strip = Strip {
            min: v,
            max: v,
            is_start: false,
        };
    }
}

/// `TableLayout.xDistributeSize` for the min (`use_max` false) or max size.
fn distribute_one(
    styles: &[Track],
    strips: &mut [Strip],
    start: usize,
    stop: usize,
    desired: i32,
    use_max: bool,
) {
    let get = |s: &Strip| if use_max { s.max } else { s.min };
    let set = |s: &mut Strip, v: i32| {
        if use_max { s.max = v } else { s.min = v }
    };
    let desired = desired.max(0);
    let mut current = 0;
    let mut uninitialized = 0;
    for (i, strip) in strips.iter().enumerate().take(stop).skip(start) {
        if !is_absolute(styles, i) && get(strip) == 0 {
            uninitialized += 1;
        }
        current += get(strip);
    }
    let missing = desired - current;
    if missing <= 0 {
        return;
    }
    if uninitialized == 0 {
        let mut stop = stop;
        let last_percent = (start..stop)
            .rev()
            .find(|&i| matches!(styles.get(i), Some(Track::Percent(_))));
        if let Some(lp) = last_percent {
            stop = lp + 1;
        }
        for i in (start..stop).rev() {
            if !is_absolute(styles, i) {
                if i != strips.len() - 1 && !strips[i + 1].is_start && !is_absolute(styles, i + 1) {
                    let next = get(&strips[i + 1]);
                    let offset = next.min(missing);
                    set(&mut strips[i + 1], next - offset);
                }
                let v = get(&strips[i]) + missing;
                set(&mut strips[i], v);
                break;
            }
        }
    } else {
        let mut average = missing / uninitialized;
        let mut index = 0;
        for (i, strip) in strips.iter_mut().enumerate().take(stop).skip(start) {
            if !is_absolute(styles, i) && get(strip) == 0 {
                index += 1;
                if index == uninitialized {
                    average = missing - average * (uninitialized - 1);
                }
                let v = get(strip) + average;
                set(strip, v);
            }
        }
    }
}

/// `TableLayout.DistributeStyles`.
fn distribute_styles(
    styles: &[Track],
    strips: &mut [Strip],
    max_size: i32,
    dont_honor: bool,
) -> i32 {
    let mut used = 0;
    let mut total_percent = 0f32;
    let mut percent_allocated = 0f32;
    let mut abs_auto_allocated = 0f32;
    let mut has_auto = false;
    for (i, strip) in strips.iter().enumerate() {
        match styles.get(i) {
            Some(Track::Absolute(_)) => abs_auto_allocated += strip.min as f32,
            Some(Track::Percent(p)) => {
                total_percent += p;
                percent_allocated += strip.min as f32;
            }
            Some(Track::AutoSize) => {
                abs_auto_allocated += strip.min as f32;
                has_auto = true;
            }
            None => has_auto = true,
        }
        used += strip.min;
    }
    let remaining = max_size - used;
    if total_percent > 0.0 {
        if !dont_honor {
            if percent_allocated > max_size as f32 - abs_auto_allocated {
                percent_allocated = (max_size as f32 - abs_auto_allocated).max(0.0);
            }
            if remaining > 0 {
                percent_allocated += remaining as f32;
            } else if remaining < 0 {
                percent_allocated = max_size as f32 - abs_auto_allocated;
            }
            for (i, strip) in strips.iter_mut().enumerate() {
                if let Some(Track::Percent(p)) = styles.get(i) {
                    let size = (p * percent_allocated / total_percent) as i32;
                    used -= strip.min;
                    used += size;
                    strip.min = size;
                }
            }
        } else {
            let mut max_percent_width = 0;
            for (i, strip) in strips.iter().enumerate() {
                if let Some(Track::Percent(p)) = styles.get(i) {
                    let total = ((strip.min as f32 * total_percent) / p).round() as i32;
                    max_percent_width = max_percent_width.max(total);
                    used -= strip.min;
                }
            }
            used += max_percent_width;
        }
    }
    let mut remaining = max_size - used;
    if has_auto && remaining > 0 {
        for (i, strip) in strips.iter_mut().enumerate() {
            if matches!(styles.get(i), Some(Track::AutoSize) | None) {
                let delta = (strip.max - strip.min).min(remaining);
                if delta > 0 {
                    used += delta;
                    remaining -= delta;
                    strip.min += delta;
                }
            }
        }
    }
    used
}

fn table_parts(node: &Node) -> (&[Track], &[Track], &[Node]) {
    match &node.kind {
        Kind::Table { cols, rows, cells } => (cols, rows, cells),
        _ => (&[], &[], &[]),
    }
}

fn table_info<'a>(cols: &'a [Track], rows: &'a [Track], cells: &[Node]) -> TableInfo<'a> {
    let max_col = cells
        .iter()
        .filter(|c| c.visible)
        .map(|c| c.col + c.col_span)
        .max()
        .unwrap_or(0);
    let max_row = cells
        .iter()
        .filter(|c| c.visible)
        .map(|c| c.row + 1)
        .max()
        .unwrap_or(0);
    TableInfo {
        cols,
        rows,
        col_strips: vec![Strip::default(); cols.len().max(max_col).max(1)],
        row_strips: vec![Strip::default(); rows.len().max(max_row).max(1)],
        cells: (0..cells.len()).filter(|&i| cells[i].visible).collect(),
    }
}

/// `TableLayout.ApplyStyles`: sizes every strip; returns the used size.
fn apply_styles(
    info: &mut TableInfo<'_>,
    cells: &[Node],
    proposed: Size,
    measure_only: bool,
    honor_override: bool,
    measure: &mut Measure<'_>,
) -> Size {
    init_strips(info.cols, &mut info.col_strips);
    init_strips(info.rows, &mut info.row_strips);
    let mut has_col_span = false;
    for &i in &info.cells {
        info.col_strips[cells[i].col].is_start = true;
        info.row_strips[cells[i].row].is_start = true;
        has_col_span |= cells[i].col_span > 1;
    }

    // InflateColumns
    let dont_honor_cols = measure_only && !honor_override;
    if has_col_span {
        info.cells.sort_by_key(|&i| cells[i].col_span);
    }
    for &i in &info.cells.clone() {
        let c = &cells[i];
        if c.col_span > 1 || !is_absolute(info.cols, c.col) {
            let (min_w, max_w) = if c.col_span == 1 && is_absolute(info.rows, c.row) {
                let h = match info.rows[c.row] {
                    Track::Absolute(v) => v,
                    _ => 0,
                };
                let w = element_size(c, Size { w: 0, h }, measure).w;
                (w, w)
            } else {
                (
                    element_size(c, Size { w: 1, h: 0 }, measure).w,
                    element_size(c, Size::default(), measure).w,
                )
            };
            let stop = (c.col + c.col_span).min(info.col_strips.len());
            let m = c.margin.horizontal();
            distribute_one(
                info.cols,
                &mut info.col_strips,
                c.col,
                stop,
                min_w + m,
                false,
            );
            distribute_one(
                info.cols,
                &mut info.col_strips,
                c.col,
                stop,
                max_w + m,
                true,
            );
        }
    }
    let mut width = distribute_styles(info.cols, &mut info.col_strips, proposed.w, dont_honor_cols);
    if dont_honor_cols && width > proposed.w && proposed.w > 1 {
        let mut total_percent = 0f32;
        let mut percent_space = 0;
        for (i, s) in info.col_strips.iter().enumerate() {
            if let Some(Track::Percent(p)) = info.cols.get(i) {
                total_percent += p;
                percent_space += s.min;
            }
        }
        let steal = (width - proposed.w).min(percent_space);
        for (i, s) in info.col_strips.iter_mut().enumerate() {
            if let Some(Track::Percent(p)) = info.cols.get(i) {
                s.min -= (p / total_percent * steal as f32) as i32;
            }
        }
        width -= steal;
    }

    // InflateRows
    let expand_last = (proposed.w - width).max(0);
    let multiple_percent = info
        .cols
        .iter()
        .filter(|t| matches!(t, Track::Percent(_)))
        .count()
        > 1;
    let dont_honor_rows = measure_only && !honor_override;
    let max_columns = info.col_strips.len();
    for &i in &info.cells {
        let c = &cells[i];
        if !is_absolute(info.rows, c.row) {
            let mut current = info.col_strips[c.col..(c.col + c.col_span).min(max_columns)]
                .iter()
                .map(|s| s.min)
                .sum::<i32>();
            if !dont_honor_rows && c.col + c.col_span >= max_columns && !multiple_percent {
                current += expand_last;
            }
            let m = c.margin;
            let h = element_size(
                c,
                Size {
                    w: current - m.horizontal(),
                    h: 0,
                },
                measure,
            )
            .h + m.vertical();
            let stop = (c.row + 1).min(info.row_strips.len());
            distribute_one(info.rows, &mut info.row_strips, c.row, stop, h, false);
            distribute_one(info.rows, &mut info.row_strips, c.row, stop, h, true);
        }
    }
    let height = distribute_styles(info.rows, &mut info.row_strips, proposed.h, dont_honor_rows);
    Size {
        w: width,
        h: height,
    }
}

fn table_preferred(node: &Node, proposed: Size, measure: &mut Measure<'_>) -> Size {
    let (cols, rows, cells) = table_parts(node);
    let mut info = table_info(cols, rows, cells);
    let proposed = Size {
        w: proposed.w.max(1),
        h: proposed.h.max(1),
    };
    // C# parity: InflateColumns honors the constraint for a docked Fill/Top table in a Panel.
    let honor = proposed.w < i32::from(i16::MAX) && matches!(node.dock, Dock::Fill | Dock::Top);
    apply_styles(&mut info, cells, proposed, true, honor, measure)
}

fn arrange_table(node: &mut Node, display: Rect, measure: &mut Measure<'_>) {
    let container = Size {
        w: display.w.max(1),
        h: display.h.max(1),
    };
    let placements = {
        let (cols, rows, cells) = table_parts(node);
        let mut info = table_info(cols, rows, cells);
        let used = apply_styles(&mut info, cells, container, false, false, measure);
        // ExpandLastElement
        if let Some(last) = info.col_strips.last_mut()
            && container.w > used.w
        {
            last.min += container.w - used.w;
        }
        if let Some(last) = info.row_strips.last_mut()
            && container.h > used.h
        {
            last.min += container.h - used.h;
        }
        // SetElementBounds: cells sorted by row, then column.
        info.cells.sort_by_key(|&i| (cells[i].row, cells[i].col));
        let mut out = Vec::with_capacity(info.cells.len());
        for &i in &info.cells {
            let c = &cells[i];
            let x = display.x + info.col_strips[..c.col].iter().map(|s| s.min).sum::<i32>();
            let y = display.y + info.row_strips[..c.row].iter().map(|s| s.min).sum::<i32>();
            let w = info.col_strips[c.col..(c.col + c.col_span).min(info.col_strips.len())]
                .iter()
                .map(|s| s.min)
                .sum::<i32>();
            let h = info.row_strips[c.row].min;
            let mut cell = Rect { x, y, w, h }.deflate(c.margin);
            cell.w = cell.w.max(1);
            cell.h = cell.h.max(1);
            let size = element_size(c, cell.size(), measure);
            let mut r = align_and_stretch(size, cell, unified_anchor(c));
            r.w = r.w.min(cell.w);
            r.h = r.h.min(cell.h);
            out.push((i, r));
        }
        out
    };
    let cells = node.children_mut();
    for (i, r) in placements {
        arrange(&mut cells[i], r, measure);
    }
}
