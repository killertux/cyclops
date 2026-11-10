//! The map: which tiles it shows, and where each one is drawn. See `specs/tui/map.md`.
//!
//! The layout is worked out here, once, so that drawing it and moving through it agree.

use super::{App, SIDE_BY_SIDE, parent_of};
use crate::canvas::inner;
use crate::treemap::squarify;
use cyclops_domain::metric::{MetricId, Subject};
use cyclops_domain::symbol::{Symbol, SymbolId, SymbolKind};
use ratatui::layout::Rect;
use std::collections::HashMap;

/// The map, laid out on the screen.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MapScene {
    /// The title of the one pane, when the map shows the symbols inside a file.
    pub title: Option<String>,
    /// A frame for each package, when the map shows files.
    pub frames: Vec<MapFrame>,
    /// A group for each module that holds files of its own.
    pub groups: Vec<MapGroup>,
    pub tiles: Vec<MapTile>,
}

/// A package's frame: its box and its title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapFrame {
    pub id: SymbolId,
    pub area: Rect,
    pub title: String,
}

/// A module's group: its name sits on the first row of its area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapGroup {
    pub id: SymbolId,
    pub name: String,
    pub area: Rect,
}

/// One tile, with the space it fills. A `+N small` tile stands for the tiles folded
/// into it, and its id is no symbol's.
#[derive(Debug, Clone, PartialEq)]
pub struct MapTile {
    pub id: SymbolId,
    pub name: String,
    pub lines: f64,
    pub area: Rect,
    pub folded: Vec<SymbolId>,
}

/// The least room a tile needs to show its name and its value.
const TILE_WIDTH: u16 = 6;
const TILE_HEIGHT: u16 = 2;

impl MapScene {
    /// How many tiles the scene stands for, counting those folded away.
    #[must_use]
    pub fn tile_count(&self) -> usize {
        self.tiles.iter().map(|tile| tile.folded.len().max(1)).sum()
    }

    /// The `+N small` tile with this id, if it is one.
    #[must_use]
    pub fn small(&self, id: &SymbolId) -> Option<&MapTile> {
        self.tiles
            .iter()
            .find(|tile| &tile.id == id && !tile.folded.is_empty())
    }
}

/// The packages' frames, laid out: a slot for each package left, and the band of the
/// `+N small packages` tile with the packages folded into it and their lines.
struct FoldedPackages {
    slots: Vec<Rect>,
    band: Option<Rect>,
    folded: Vec<(SymbolId, f64)>,
}

/// What the map knows of a tile before laying it out.
struct Facts<'a> {
    symbol: &'a Symbol,
    name: String,
    lines: f64,
    frame: Option<SymbolId>,
    group: Option<SymbolId>,
}

/// The files of one module: their code lines, their name, and their lines in each
/// language.
#[derive(Default)]
struct Files<'a> {
    code: u32,
    name: Option<&'a str>,
    languages: Vec<(&'static str, u32)>,
}

/// A direction to move in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toward {
    Left,
    Right,
    Up,
    Down,
}

impl App {
    /// How many tiles the map would show without the filter.
    #[must_use]
    pub fn unfiltered_tiles(&self) -> usize {
        self.map_contents().len()
    }

    /// The lines of a tile, which its area follows: a file's lines of code, or else the
    /// symbol's.
    #[must_use]
    pub fn tile_lines(&self, id: &SymbolId) -> f64 {
        self.facts()
            .iter()
            .find(|facts| &facts.symbol.id == id)
            .map(|facts| facts.lines)
            .or_else(|| {
                self.analysis.as_ref().and_then(|analysis| {
                    analysis.value(&Subject::Symbol(id.clone()), MetricId::Lines)
                })
            })
            .unwrap_or(1.0)
    }

    /// The tile with the most lines.
    #[must_use]
    pub fn largest_tile(&self) -> Option<SymbolId> {
        self.facts()
            .into_iter()
            .max_by(|a, b| {
                a.lines
                    .total_cmp(&b.lines)
                    .then_with(|| b.symbol.id.cmp(&a.symbol.id))
            })
            .map(|facts| facts.symbol.id.clone())
    }

    /// The tiles of the map: what is inside the focus, or inside its parent when the
    /// focus has nothing inside it.
    #[must_use]
    pub fn tiles(&self) -> Vec<&Symbol> {
        let kept = self.kept();
        let mut tiles: Vec<&Symbol> = self
            .map_contents()
            .into_iter()
            .filter(|symbol| kept.as_ref().is_none_or(|kept| kept.contains(&symbol.id)))
            .collect();
        tiles.sort_by(|a, b| a.id.cmp(&b.id));
        tiles.dedup_by(|a, b| a.id == b.id);
        tiles
    }

    /// The group a file's tile sits in: the module that holds it, unless that is the
    /// holder or a package.
    #[must_use]
    pub fn tile_group(&self, tile: &SymbolId) -> Option<SymbolId> {
        self.facts()
            .into_iter()
            .find(|facts| &facts.symbol.id == tile)
            .and_then(|facts| facts.group)
    }

    /// The groups the map shows, in order.
    #[must_use]
    pub fn map_groups(&self) -> Vec<SymbolId> {
        let mut groups: Vec<SymbolId> = self
            .facts()
            .into_iter()
            .filter_map(|facts| facts.group)
            .collect();
        groups.sort();
        groups.dedup();
        groups
    }

    /// The package a file's tile is framed in, when the map shows files.
    #[must_use]
    pub fn tile_frame(&self, tile: &SymbolId) -> Option<SymbolId> {
        self.facts()
            .into_iter()
            .find(|facts| &facts.symbol.id == tile)
            .and_then(|facts| facts.frame)
    }

    /// The name of a tile: the file's name for a file, and the symbol's otherwise.
    #[must_use]
    pub fn tile_name(&self, tile: &SymbolId) -> String {
        self.facts()
            .into_iter()
            .find(|facts| &facts.symbol.id == tile)
            .map_or_else(
                || {
                    self.symbol(tile)
                        .map(|symbol| symbol.name.clone())
                        .unwrap_or_default()
                },
                |facts| facts.name,
            )
    }

    /// Where the map is drawn on the screen: the view's part of the body, above the
    /// legend.
    #[must_use]
    pub fn map_area(&self) -> Rect {
        let mut area = Rect {
            x: 0,
            y: 2,
            width: self.columns,
            height: self.rows.saturating_sub(4),
        };
        if self.inspector.is_some() && area.width >= SIDE_BY_SIDE {
            let side = self
                .inspector_width_for(36)
                .min(area.width.saturating_sub(20));
            area.width = area.width.saturating_sub(side);
        }
        area.height = area.height.saturating_sub(1);
        area
    }

    /// The map laid out in `area`: frames for packages, groups for modules, and tiles.
    #[must_use]
    pub fn map_scene(&self, area: Rect) -> MapScene {
        let facts = self.facts();
        let framed = facts.first().is_some_and(|facts| facts.frame.is_some());
        if !framed {
            let title = self
                .map_holder()
                .and_then(|holder| self.symbol(&holder))
                .map_or_else(
                    || "Map".to_owned(),
                    |symbol| format!("Map of {}", symbol.name),
                );
            let mut scene = MapScene {
                title: Some(title),
                ..MapScene::default()
            };
            lay_tiles(&mut scene, &facts, inner(area));
            return scene;
        }
        let mut scene = MapScene::default();
        // Each package is a frame, with its share of the lines.
        let mut packages: Vec<(SymbolId, Vec<&Facts<'_>>)> = Vec::new();
        for tile in &facts {
            let Some(frame) = &tile.frame else {
                continue;
            };
            match packages.iter_mut().find(|(id, _)| id == frame) {
                Some((_, members)) => members.push(tile),
                None => packages.push((frame.clone(), vec![tile])),
            }
        }
        let weights: Vec<f64> = packages
            .iter()
            .map(|(_, members)| members.iter().map(|tile| tile.lines).sum())
            .collect();
        let languages = self.languages_by_module();
        let FoldedPackages {
            slots,
            band,
            folded,
        } = fold_packages(&mut packages, weights, area, |id| {
            self.symbol(id)
                .map_or(0, |symbol| symbol.name.chars().count())
        });
        for ((frame, members), slot) in packages.iter().zip(slots) {
            if slot.width < 3 || slot.height < 3 {
                continue;
            }
            let title = self.frame_title(frame, members, &languages, slot.width);
            scene.frames.push(MapFrame {
                id: frame.clone(),
                area: slot,
                title,
            });
            self.lay_frame(&mut scene, members, inner(slot));
        }
        if let Some(band) = band {
            let lines = folded.iter().map(|(_, lines)| lines).sum();
            let mut ids: Vec<SymbolId> = folded.into_iter().map(|(id, _)| id).collect();
            ids.sort();
            scene.tiles.push(MapTile {
                id: SymbolId::new(&format!("+packages {} {}", band.x, band.y)),
                name: format!("+{} small packages", ids.len()),
                lines,
                area: gap(band),
                folded: ids,
            });
        }
        scene
    }

    /// The tile `toward` the selected one: the nearest whose side faces it, with tiles
    /// in line counting nearer than tiles off to the side. Rows count double, since a
    /// cell is about twice as tall as it is wide.
    pub(super) fn map_move(&mut self, toward: Toward) {
        let scene = self.map_scene(self.map_area());
        let here = self
            .map_selected
            .as_ref()
            .and_then(|selected| scene.tiles.iter().find(|tile| &tile.id == selected));
        let Some(here) = here else {
            self.map_selected = scene.tiles.first().map(|tile| tile.id.clone());
            return;
        };
        let from = here.area;
        let next = scene
            .tiles
            .iter()
            .filter(|tile| tile.id != here.id)
            .filter_map(|tile| distance(from, tile.area, toward).map(|score| (score, tile)))
            .min_by(|(a, one), (b, other)| a.cmp(b).then_with(|| one.id.cmp(&other.id)));
        if let Some((_, tile)) = next {
            self.map_selected = Some(tile.id.clone());
        }
    }

    /// What the map shows before the filter: the files inside its holder, each as its
    /// module, or the holder's symbols when it holds no files.
    pub(super) fn map_contents(&self) -> Vec<&Symbol> {
        let (Some(analysis), Some(holder)) = (&self.analysis, self.map_holder()) else {
            return Vec::new();
        };
        let files = self.map_files(&holder);
        if files.is_empty() {
            analysis.view().children(&holder).collect()
        } else {
            files
        }
    }

    /// The symbol the map is drawn inside.
    pub(super) fn map_holder(&self) -> Option<SymbolId> {
        let analysis = self.analysis.as_ref()?;
        let focus = self.focus.as_ref()?;
        if analysis.view().children(focus).next().is_some() {
            return Some(focus.clone());
        }
        analysis
            .view()
            .symbol(focus)
            .and_then(parent_of)
            .cloned()
            .or_else(|| Some(focus.clone()))
    }

    /// The packages the map is drawn for: every package of a project, every package at
    /// the top, or the holder alone below them.
    fn map_scope(&self, holder: &SymbolId) -> Vec<SymbolId> {
        let Some(analysis) = &self.analysis else {
            return Vec::new();
        };
        let Some(symbol) = analysis.view().symbol(holder) else {
            return vec![holder.clone()];
        };
        let tops = if symbol.kind == SymbolKind::Project {
            analysis.children_by(holder, MetricId::Lines)
        } else if symbol.parent.is_none() {
            analysis.roots_by(MetricId::Lines)
        } else {
            return vec![holder.clone()];
        };
        tops.iter().map(|symbol| symbol.id.clone()).collect()
    }

    /// The modules of the files inside the holder. A file that is the holder itself is
    /// the frame around the others, not a tile.
    fn map_files(&self, holder: &SymbolId) -> Vec<&Symbol> {
        let Some(analysis) = &self.analysis else {
            return Vec::new();
        };
        let view = analysis.view();
        let scope = self.map_scope(holder);
        analysis
            .files()
            .iter()
            .filter_map(|file| file.module.as_ref())
            .filter(|module| {
                scope
                    .iter()
                    .any(|top| *module != top && view.is_within(module, top))
            })
            .filter_map(|module| view.symbol(module))
            .collect()
    }

    /// Everything the map needs to know of each tile, found in one pass over the files.
    fn facts(&self) -> Vec<Facts<'_>> {
        let (Some(analysis), Some(holder)) = (&self.analysis, self.map_holder()) else {
            return Vec::new();
        };
        let view = analysis.view();
        let shows_files = !self.map_files(&holder).is_empty();
        let scope = if shows_files {
            self.map_scope(&holder)
        } else {
            Vec::new()
        };
        let files = self.files_by_module();
        self.tiles()
            .into_iter()
            .map(|symbol| {
                let held = files.get(&symbol.id);
                let lines = match held {
                    Some(held) if held.code > 0 => f64::from(held.code),
                    _ => analysis
                        .value(&Subject::Symbol(symbol.id.clone()), MetricId::Lines)
                        .unwrap_or(1.0)
                        .max(1.0),
                };
                let frame = scope
                    .iter()
                    .find(|top| view.is_within(&symbol.id, top))
                    .cloned();
                let group = frame.as_ref().and_then(|_| {
                    let group = view.symbol(parent_of(symbol)?)?;
                    (group.kind != SymbolKind::Package && !scope.contains(&group.id))
                        .then(|| group.id.clone())
                });
                let name = match held.and_then(|held| held.name) {
                    Some(name) if frame.is_some() => name.to_owned(),
                    _ => symbol.name.clone(),
                };
                Facts {
                    symbol,
                    name,
                    lines,
                    frame,
                    group,
                }
            })
            .collect()
    }

    /// The files of each module, as far as the views show them.
    fn files_by_module(&self) -> HashMap<&SymbolId, Files<'_>> {
        let mut by_module: HashMap<&SymbolId, Files<'_>> = HashMap::new();
        let Some(analysis) = &self.analysis else {
            return by_module;
        };
        let tests = analysis.tests();
        for file in analysis.files() {
            let Some(module) = &file.module else {
                continue;
            };
            let entry = by_module.entry(module).or_default();
            if entry.name.is_none() {
                entry.name = file.path.as_str().rsplit('/').next();
            }
            if !tests.shows(file.role) {
                continue;
            }
            let code = file.counted_lines(tests).code;
            entry.code += code;
            let language = file.language.name();
            match entry
                .languages
                .iter_mut()
                .find(|(name, _)| *name == language)
            {
                Some((_, total)) => *total += code,
                None => entry.languages.push((language, code)),
            }
        }
        by_module
    }

    /// The lines of each module in each language.
    fn languages_by_module(&self) -> HashMap<SymbolId, Vec<(&'static str, u32)>> {
        self.files_by_module()
            .into_iter()
            .map(|(module, files)| (module.clone(), files.languages))
            .collect()
    }

    /// A package's frame title: its name, main language and lines, or only its name and
    /// lines where the frame is narrow.
    fn frame_title(
        &self,
        frame: &SymbolId,
        members: &[&Facts<'_>],
        languages: &HashMap<SymbolId, Vec<(&'static str, u32)>>,
        width: u16,
    ) -> String {
        let name = self
            .symbol(frame)
            .map(|symbol| symbol.name.clone())
            .unwrap_or_default();
        let lines: f64 = members.iter().map(|tile| tile.lines).sum();
        let lines = crate::state::shown(MetricId::Lines, lines);
        let mut by_language: Vec<(&'static str, u32)> = Vec::new();
        for (language, code) in members
            .iter()
            .filter_map(|tile| languages.get(&tile.symbol.id))
            .flatten()
        {
            match by_language.iter_mut().find(|(name, _)| name == language) {
                Some((_, total)) => *total += code,
                None => by_language.push((language, *code)),
            }
        }
        let language = by_language
            .into_iter()
            .max_by_key(|(_, total)| *total)
            .map(|(language, _)| language);
        // The title gives up the language, then the lines, rather than be cut short.
        let room = usize::from(width.saturating_sub(6));
        let short = format!("{name}  {lines}");
        match language {
            Some(language) if width >= 40 => format!("{name}  {language}  {lines} lines"),
            _ if short.chars().count() <= room => short,
            _ => name,
        }
    }

    /// What a frame holds: groups with their tiles, and tiles of their own.
    fn lay_frame(&self, scene: &mut MapScene, members: &[&Facts<'_>], inside: Rect) {
        let mut units: Vec<(Option<&SymbolId>, Vec<&Facts<'_>>)> = Vec::new();
        for tile in members {
            let group = tile.group.as_ref();
            // Files with no group of their own are laid out together, so that the small
            // ones fold into one tile.
            match units.iter_mut().find(|(id, _)| *id == group) {
                Some((_, tiles)) => tiles.push(tile),
                None => units.push((group, vec![tile])),
            }
        }
        let weights: Vec<f64> = units
            .iter()
            .map(|(_, tiles)| tiles.iter().map(|tile| tile.lines).sum())
            .collect();
        for ((group, tiles), slot) in units.iter().zip(squarify(&weights, inside)) {
            let Some(group) = group else {
                lay_tiles(scene, tiles, slot);
                continue;
            };
            let name = self
                .symbol(group)
                .map(|symbol| symbol.name.clone())
                .unwrap_or_default();
            scene.groups.push(MapGroup {
                id: (*group).clone(),
                name,
                area: slot,
            });
            let room = Rect {
                x: slot.x + 1,
                y: slot.y + 1,
                width: slot.width.saturating_sub(1),
                height: slot.height.saturating_sub(1),
            };
            lay_tiles(scene, tiles, room);
        }
    }
}

/// Lays tiles out in `area`, each leaving a column and a row free beside it. Tiles too
/// small to show their name and value are folded into one `+N small` tile, along the
/// bottom of the area, or its right side where it is low.
fn lay_tiles<'a, T>(scene: &mut MapScene, tiles: &[T], area: Rect)
where
    T: std::borrow::Borrow<Facts<'a>>,
{
    // An area with no room, such as a package the screen had no place for, shows nothing.
    if area.width == 0 || area.height == 0 {
        return;
    }
    let mut shown: Vec<&Facts<'a>> = tiles.iter().map(std::borrow::Borrow::borrow).collect();
    let mut folded: Vec<&Facts<'a>> = Vec::new();
    let (slots, band) = loop {
        let (room, band) = if folded.is_empty() {
            (area, None)
        } else {
            let (room, band) = fold_band(area);
            (room, Some(band))
        };
        let weights: Vec<f64> = shown.iter().map(|tile| tile.lines).collect();
        let slots: Vec<Rect> = squarify(&weights, room).into_iter().map(gap).collect();
        let small: Vec<usize> = slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.width < TILE_WIDTH || slot.height < TILE_HEIGHT)
            .map(|(index, _)| index)
            .collect();
        if small.is_empty() {
            break (slots, band);
        }
        for index in small.into_iter().rev() {
            if index < shown.len() {
                folded.push(shown.remove(index));
            }
        }
    };
    for (tile, area) in shown.iter().zip(&slots) {
        scene.tiles.push(MapTile {
            id: tile.symbol.id.clone(),
            name: tile.name.clone(),
            lines: tile.lines,
            area: *area,
            folded: Vec::new(),
        });
    }
    // With nothing left to show, the `+N small` tile takes the whole area.
    let band = band.map(|band| if shown.is_empty() { area } else { band });
    if let Some(band) = band {
        let mut ids: Vec<SymbolId> = folded.iter().map(|tile| tile.symbol.id.clone()).collect();
        ids.sort();
        scene.tiles.push(MapTile {
            id: SymbolId::new(&format!("+small {} {}", band.x, band.y)),
            name: format!("+{} small", ids.len()),
            lines: folded.iter().map(|tile| tile.lines).sum(),
            area: gap(band),
            folded: ids,
        });
    }
}

/// Lays the packages' frames out in `area`. A frame with no room for its title and one
/// tile is folded, with the others like it, into a band along the bottom. Gives the
/// frames of the packages left, the band, and the packages folded with their lines.
fn fold_packages<T>(
    packages: &mut Vec<(SymbolId, T)>,
    mut weights: Vec<f64>,
    area: Rect,
    name: impl Fn(&SymbolId) -> usize,
) -> FoldedPackages {
    let mut folded: Vec<(SymbolId, f64)> = Vec::new();
    loop {
        let (room, band) = if folded.is_empty() {
            (area, None)
        } else {
            let (room, band) = fold_band(area);
            (room, Some(band))
        };
        let slots = squarify(&weights, room);
        let small: Vec<usize> = slots
            .iter()
            .zip(packages.iter())
            .enumerate()
            .filter(|(_, (slot, (id, _)))| !frame_fits(name(id), **slot))
            .map(|(index, _)| index)
            .collect();
        if small.is_empty() {
            // With nothing left to frame, the folded packages take the whole area.
            let band = band.map(|band| if packages.is_empty() { area } else { band });
            return FoldedPackages {
                slots,
                band,
                folded,
            };
        }
        for index in small.into_iter().rev() {
            if index < packages.len() {
                let (id, _) = packages.remove(index);
                let lines = weights.remove(index);
                folded.push((id, lines));
            }
        }
    }
}

/// Whether a package's frame has room for its title, a name this long, and one tile.
fn frame_fits(name: usize, slot: Rect) -> bool {
    let title = u16::try_from(name).unwrap_or(u16::MAX).saturating_add(6);
    slot.width >= title.max(TILE_WIDTH + 3) && slot.height >= TILE_HEIGHT + 3
}

/// Splits an area into the room left for tiles and a band for the `+N small` tile: a row
/// of tiles at the bottom, or a column on the right where the area is low, or the whole
/// area where it has no room to share.
fn fold_band(area: Rect) -> (Rect, Rect) {
    let tall = TILE_HEIGHT + 1;
    let wide = TILE_WIDTH * 2;
    if area.height >= tall * 2 {
        let room = Rect {
            height: area.height - tall,
            ..area
        };
        let band = Rect {
            y: area.y + area.height - tall,
            height: tall,
            ..area
        };
        (room, band)
    } else if area.width >= wide * 2 {
        let room = Rect {
            width: area.width - wide,
            ..area
        };
        let band = Rect {
            x: area.x + area.width - wide,
            width: wide,
            ..area
        };
        (room, band)
    } else {
        (
            Rect {
                width: 0,
                height: 0,
                ..area
            },
            area,
        )
    }
}

/// A tile's area in its slot, which leaves a column and a row free beside it.
fn gap(slot: Rect) -> Rect {
    Rect {
        width: slot.width.saturating_sub(1).max(1),
        height: slot.height.saturating_sub(1).max(1),
        ..slot
    }
}

/// How far `to` is from `from` going `toward`, or nothing when it is not that way.
fn distance(from: Rect, to: Rect, toward: Toward) -> Option<(u32, u32)> {
    let span = |start: u16, length: u16| (u32::from(start), u32::from(start) + u32::from(length));
    let (from_x, to_x) = (span(from.x, from.width), span(to.x, to.width));
    let (from_y, to_y) = (span(from.y, from.height), span(to.y, to.height));
    // The gap between two ranges, or nothing when they overlap.
    let gap = |(start, end): (u32, u32), (other_start, other_end): (u32, u32)| {
        if other_end <= start {
            start - other_end
        } else {
            other_start.saturating_sub(end)
        }
    };
    let center = |(start, end): (u32, u32)| start + end;
    let (ahead, aside, off_center) = match toward {
        Toward::Right if to_x.0 >= from_x.1 => (
            to_x.0 - from_x.1,
            gap(from_y, to_y) * 2,
            center(from_y).abs_diff(center(to_y)),
        ),
        Toward::Left if to_x.1 <= from_x.0 => (
            from_x.0 - to_x.1,
            gap(from_y, to_y) * 2,
            center(from_y).abs_diff(center(to_y)),
        ),
        Toward::Down if to_y.0 >= from_y.1 => (
            (to_y.0 - from_y.1) * 2,
            gap(from_x, to_x),
            center(from_x).abs_diff(center(to_x)),
        ),
        Toward::Up if to_y.1 <= from_y.0 => (
            (from_y.0 - to_y.1) * 2,
            gap(from_x, to_x),
            center(from_x).abs_diff(center(to_x)),
        ),
        _ => return None,
    };
    Some((ahead + aside * 2, off_center))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_too_small_to_read_fold_into_one() {
        use cyclops_domain::symbol::CodeRole;
        let symbol = |name: &str| {
            Symbol::container(
                SymbolId::new(name),
                None,
                name,
                SymbolKind::Module,
                CodeRole::Production,
            )
        };
        let symbols: Vec<Symbol> = ["big", "t1", "t2", "t3"].into_iter().map(symbol).collect();
        let facts: Vec<Facts<'_>> = symbols
            .iter()
            .zip([480.0, 1.0, 1.0, 1.0])
            .map(|(symbol, lines)| Facts {
                symbol,
                name: symbol.name.clone(),
                lines,
                frame: None,
                group: None,
            })
            .collect();
        let mut scene = MapScene::default();
        lay_tiles(&mut scene, &facts, Rect::new(0, 0, 78, 17));
        let names: Vec<&str> = scene.tiles.iter().map(|tile| tile.name.as_str()).collect();
        assert_eq!(names, ["big", "+3 small"], "{scene:?}");
        assert_eq!(scene.tile_count(), 4);
        let mut alone = MapScene::default();
        lay_tiles(&mut alone, &facts[1..], Rect::new(0, 0, 4, 3));
        let all = alone
            .tiles
            .first()
            .map(|tile| (tile.name.as_str(), tile.area));
        assert_eq!(all, Some(("+3 small", Rect::new(0, 0, 3, 2))));
        let mut nowhere = MapScene::default();
        lay_tiles(&mut nowhere, &facts, Rect::new(1, 1, 0, 0));
        assert!(nowhere.tiles.is_empty(), "{nowhere:?}");
        assert!(scene.tiles.iter().all(|tile| tile.area.width >= TILE_WIDTH));
    }

    #[test]
    fn packages_with_no_room_for_a_frame_fold_into_one() {
        let ids = ["engine", "c01", "c02", "c03"].map(SymbolId::new);
        let mut packages: Vec<(SymbolId, ())> = ids.iter().map(|id| (id.clone(), ())).collect();
        let area = Rect::new(0, 2, 80, 17);
        let folded = fold_packages(&mut packages, vec![300.0, 2.0, 2.0, 2.0], area, |id| {
            id.as_str().len()
        });
        let left: Vec<&str> = packages.iter().map(|(id, ())| id.as_str()).collect();
        assert_eq!(left, ["engine"]);
        let mut names: Vec<&str> = folded.folded.iter().map(|(id, _)| id.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, ["c01", "c02", "c03"]);
        assert_eq!(folded.band, Some(Rect::new(0, 16, 80, 3)));
        assert!(folded.slots.iter().all(|slot| slot.y + slot.height <= 16));
        let mut alone: Vec<(SymbolId, ())> = vec![(SymbolId::new("c01"), ())];
        let tiny = Rect::new(0, 0, 6, 3);
        let all = fold_packages(&mut alone, vec![2.0], tiny, |id| id.as_str().len());
        assert_eq!(alone.len(), 0, "{alone:?}");
        assert_eq!(all.band, Some(tiny));
        assert!(frame_fits(3, Rect::new(0, 0, 9, 5)));
        assert!(!frame_fits(3, Rect::new(0, 0, 8, 5)));
        assert!(!frame_fits(3, Rect::new(0, 0, 9, 4)));
    }

    #[test]
    fn the_nearest_tile_that_way_is_next() {
        let here = Rect::new(0, 0, 10, 4);
        let beside = Rect::new(11, 0, 10, 4);
        let below_right = Rect::new(11, 5, 10, 4);
        let right = |to| distance(here, to, Toward::Right);
        assert!(right(beside) < right(below_right));
        assert_eq!(distance(here, beside, Toward::Left), None);
        assert_eq!(distance(here, Rect::new(0, 5, 10, 4), Toward::Right), None);
    }

    #[test]
    fn a_tile_in_line_beats_a_closer_one_off_to_the_side() {
        let here = Rect::new(0, 0, 10, 2);
        let far_in_line = Rect::new(30, 0, 10, 2);
        let near_aside = Rect::new(11, 12, 10, 2);
        let right = |to| distance(here, to, Toward::Right);
        assert!(right(far_in_line) < right(near_aside));
    }
}
